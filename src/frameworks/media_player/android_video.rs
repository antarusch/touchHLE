/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Android's native MediaExtractor + MediaCodec H.264 playback.
//! Decode into RGBA pixels for touchHLE's existing Core Animation compositor.
//! This is intentionally platform-specific: other hosts retain simulated
//! playback until a corresponding decoder is available.

use std::ffi::{c_char, c_void, CStr, CString};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering};

const TRY_AGAIN_LATER: isize = -1;
const OUTPUT_FORMAT_CHANGED: isize = -2;
const BUFFER_FLAG_END_OF_STREAM: u32 = 4;
const COLOR_YUV420_PLANAR: i32 = 19;
const COLOR_YUV420_SEMIPLANAR: i32 = 21;
const COLOR_YUV420_PACKED_SEMIPLANAR: i32 = 39;
const COLOR_QCOM_NV12_ALT: i32 = 0x7fa30c02u32 as i32;
const COLOR_YUV420_FLEXIBLE: i32 = 0x7f420888;
const COLOR_QCOM_NV12: i32 = 0x7fa30c00u32 as i32;
const COLOR_QCOM_NV12_VENUS: i32 = 0x7fa30c04u32 as i32;
const MAX_FRAME_PIXELS: usize = 4096 * 4096;

static MOVIE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[repr(C)]
struct AMediaExtractor {
    _private: [u8; 0],
}
#[repr(C)]
struct AMediaCodec {
    _private: [u8; 0],
}
#[repr(C)]
struct AMediaFormat {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Default)]
struct BufferInfo {
    offset: i32,
    size: i32,
    presentation_time_us: i64,
    flags: u32,
}

#[link(name = "mediandk")]
unsafe extern "C" {
    fn AMediaExtractor_new() -> *mut AMediaExtractor;
    fn AMediaExtractor_delete(extractor: *mut AMediaExtractor) -> i32;
    fn AMediaExtractor_setDataSourceFd(extractor: *mut AMediaExtractor, fd: i32, offset: i64, length: i64) -> i32;
    fn AMediaExtractor_getTrackCount(extractor: *mut AMediaExtractor) -> usize;
    fn AMediaExtractor_getTrackFormat(extractor: *mut AMediaExtractor, idx: usize) -> *mut AMediaFormat;
    fn AMediaExtractor_selectTrack(extractor: *mut AMediaExtractor, idx: usize) -> i32;
    fn AMediaExtractor_readSampleData(extractor: *mut AMediaExtractor, buffer: *mut u8, capacity: usize) -> isize;
    fn AMediaExtractor_getSampleTime(extractor: *mut AMediaExtractor) -> i64;
    fn AMediaExtractor_advance(extractor: *mut AMediaExtractor) -> bool;
    fn AMediaExtractor_seekTo(extractor: *mut AMediaExtractor, time_us: i64, mode: i32) -> i32;

    fn AMediaFormat_delete(format: *mut AMediaFormat);
    fn AMediaFormat_getString(format: *const AMediaFormat, name: *const c_char, out: *mut *const c_char) -> bool;
    fn AMediaFormat_getInt32(format: *const AMediaFormat, name: *const c_char, out: *mut i32) -> bool;
    fn AMediaFormat_getInt64(format: *const AMediaFormat, name: *const c_char, out: *mut i64) -> bool;
    fn AMediaFormat_setInt32(format: *mut AMediaFormat, name: *const c_char, value: i32);

    fn AMediaCodec_createDecoderByType(mime: *const c_char) -> *mut AMediaCodec;
    fn AMediaCodec_delete(codec: *mut AMediaCodec) -> i32;
    fn AMediaCodec_configure(codec: *mut AMediaCodec, format: *const AMediaFormat, surface: *mut c_void, crypto: *mut c_void, flags: u32) -> i32;
    fn AMediaCodec_start(codec: *mut AMediaCodec) -> i32;
    fn AMediaCodec_stop(codec: *mut AMediaCodec) -> i32;
    fn AMediaCodec_flush(codec: *mut AMediaCodec) -> i32;
    fn AMediaCodec_dequeueInputBuffer(codec: *mut AMediaCodec, timeout_us: i64) -> isize;
    fn AMediaCodec_getInputBuffer(codec: *mut AMediaCodec, index: usize, capacity: *mut usize) -> *mut u8;
    fn AMediaCodec_queueInputBuffer(codec: *mut AMediaCodec, index: usize, offset: usize, size: usize, time_us: i64, flags: u32) -> i32;
    fn AMediaCodec_dequeueOutputBuffer(codec: *mut AMediaCodec, info: *mut BufferInfo, timeout_us: i64) -> isize;
    fn AMediaCodec_getOutputBuffer(codec: *mut AMediaCodec, index: usize, capacity: *mut usize) -> *mut u8;
    fn AMediaCodec_getOutputFormat(codec: *mut AMediaCodec) -> *mut AMediaFormat;
    fn AMediaCodec_releaseOutputBuffer(codec: *mut AMediaCodec, index: usize, render: bool) -> i32;
}

fn format_i32(format: *const AMediaFormat, key: &'static [u8], default: i32) -> i32 {
    let mut value = default;
    unsafe { AMediaFormat_getInt32(format, key.as_ptr().cast(), &mut value); }
    value
}

fn format_i64(format: *const AMediaFormat, key: &'static [u8], default: i64) -> i64 {
    let mut value = default;
    unsafe { AMediaFormat_getInt64(format, key.as_ptr().cast(), &mut value); }
    value
}

pub struct Frame {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub time_us: i64,
}

pub struct MovieDecoder {
    extractor: *mut AMediaExtractor,
    codec: *mut AMediaCodec,
    source: File,
    source_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub duration_us: i64,
    stride: usize,
    slice_height: usize,
    color_format: i32,
    input_finished: bool,
    output_finished: bool,
    pending: Option<Frame>,
    reported_first_frame: bool,
}

impl Drop for MovieDecoder {
    fn drop(&mut self) {
        unsafe {
            if !self.codec.is_null() {
                AMediaCodec_stop(self.codec);
                AMediaCodec_delete(self.codec);
            }
            if !self.extractor.is_null() { AMediaExtractor_delete(self.extractor); }
        }
        let _ = std::fs::remove_file(&self.source_path);
    }
}

impl MovieDecoder {
    pub fn open(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 512 * 1024 * 1024 || bytes.is_empty() {
            return Err("Empty or oversized movie".into());
        }
        let id = MOVIE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "touchhle_movie_{}_{}.mp4", std::process::id(), id
        ));
        let mut source = OpenOptions::new().write(true).read(true).create_new(true)
            .open(&path).map_err(|err| format!("Movie temporary file: {err}"))?;
        if let Err(err) = source.write_all(bytes) {
            let _ = std::fs::remove_file(&path);
            return Err(format!("Movie source write: {err}"));
        }
        // Keep ownership of the file and both NDK objects so early errors
        // never leak a codec, descriptor or temporary MP4.
        let mut decoder = Self {
            extractor: ptr::null_mut(), codec: ptr::null_mut(),
            source, source_path: path, width: 0, height: 0,
            duration_us: 0, stride: 0, slice_height: 0,
            color_format: 0, input_finished: false, output_finished: false,
            pending: None, reported_first_frame: false,
        };
        decoder.initialize(bytes.len() as i64)?;
        Ok(decoder)
    }

    fn initialize(&mut self, size: i64) -> Result<(), String> {
        unsafe {
            self.extractor = AMediaExtractor_new();
            if self.extractor.is_null() { return Err("No Android MediaExtractor".into()); }
            let status = AMediaExtractor_setDataSourceFd(
                self.extractor, self.source.as_raw_fd(), 0, size
            );
            if status != 0 {
                return Err(format!("MediaExtractor setDataSourceFd: {status}"));
            }
            let tracks = AMediaExtractor_getTrackCount(self.extractor);
            for index in 0..tracks {
                let format = AMediaExtractor_getTrackFormat(self.extractor, index);
                if format.is_null() { continue; }
                let mut mime = ptr::null();
                let available = AMediaFormat_getString(format, b"mime\0".as_ptr().cast(), &mut mime);
                let name = if available && !mime.is_null() {
                    CStr::from_ptr(mime).to_string_lossy().into_owned()
                } else { String::new() };
                if !name.starts_with("video/") {
                    AMediaFormat_delete(format);
                    continue;
                }
                self.width = format_i32(format, b"width\0", 0).max(0) as u32;
                self.height = format_i32(format, b"height\0", 0).max(0) as u32;
                self.duration_us = format_i64(format, b"durationUs\0", 0);
                if self.width == 0 || self.height == 0
                    || self.width as usize * self.height as usize > MAX_FRAME_PIXELS {
                    AMediaFormat_delete(format);
                    return Err("Video dimensions not supported".into());
                }
                let c_mime = CString::new(name.as_str()).map_err(|err| err.to_string())?;
                self.codec = AMediaCodec_createDecoderByType(c_mime.as_ptr());
                if self.codec.is_null() {
                    AMediaFormat_delete(format);
                    return Err(format!("Android has no decoder for {name}"));
                }
                // Request a byte-buffer YUV420 output. Surface decoding
                // cannot be composited into touchHLE's emulated CALayer yet.
                AMediaFormat_setInt32(format, b"color-format\0".as_ptr().cast(), COLOR_YUV420_FLEXIBLE);
                let configured = AMediaCodec_configure(
                    self.codec, format, ptr::null_mut(), ptr::null_mut(), 0
                );
                AMediaFormat_delete(format);
                if configured != 0 {
                    return Err(format!("MediaCodec configure failed: {configured}"));
                }
                let selected = AMediaExtractor_selectTrack(self.extractor, index);
                if selected != 0 { return Err(format!("MediaExtractor selectTrack: {selected}")); }
                let started = AMediaCodec_start(self.codec);
                if started != 0 { return Err(format!("MediaCodec start failed: {started}")); }
                self.stride = self.width as usize;
                self.slice_height = self.height as usize;
                self.refresh_output_format();
                log!("Android MediaCodec started {name} {}x{} duration={}us",
                    self.width, self.height, self.duration_us);
                return Ok(());
            }
        }
        Err("No video track in movie".into())
    }

    pub fn restart(&mut self, position_us: i64) -> Result<(), String> {
        let status = unsafe {
            let ex = AMediaExtractor_seekTo(self.extractor, position_us.max(0), 0);
            let codec = AMediaCodec_flush(self.codec);
            (ex, codec)
        };
        if status.0 != 0 || status.1 != 0 {
            return Err(format!("Video seek failed: {:?}", status));
        }
        self.input_finished = false;
        self.output_finished = false;
        self.pending = None;
        Ok(())
    }

    fn feed_sample(&mut self) -> Result<bool, String> {
        if self.input_finished { return Ok(false); }
        let index = unsafe { AMediaCodec_dequeueInputBuffer(self.codec, 0) };
        if index < 0 { return Ok(false); }
        let mut capacity = 0usize;
        let buffer = unsafe { AMediaCodec_getInputBuffer(self.codec, index as usize, &mut capacity) };
        if buffer.is_null() { return Err("MediaCodec returned null input buffer".into()); }
        let count = unsafe { AMediaExtractor_readSampleData(self.extractor, buffer, capacity) };
        let end = count < 0;
        let pts = if end { 0 } else { unsafe { AMediaExtractor_getSampleTime(self.extractor) } };
        let status = unsafe {
            AMediaCodec_queueInputBuffer(
                self.codec, index as usize, 0, if end { 0 } else { count as usize },
                pts.max(0), if end { BUFFER_FLAG_END_OF_STREAM } else { 0 }
            )
        };
        if status != 0 { return Err(format!("MediaCodec input queue: {status}")); }
        if end {
            self.input_finished = true;
        } else {
            unsafe { AMediaExtractor_advance(self.extractor); }
        }
        Ok(true)
    }

    fn refresh_output_format(&mut self) {
        let format = unsafe { AMediaCodec_getOutputFormat(self.codec) };
        if format.is_null() { return; }
        self.stride = format_i32(format, b"stride\0", self.width as i32)
            .max(self.width as i32) as usize;
        self.slice_height = format_i32(format, b"slice-height\0", self.height as i32)
            .max(self.height as i32) as usize;
        self.color_format = format_i32(format, b"color-format\0", 0);
        log!("Android video output: format={:#x}, stride={}, sliceHeight={}, dimensions={}x{}",
            self.color_format, self.stride, self.slice_height, self.width, self.height);
        unsafe { AMediaFormat_delete(format); }
    }

    fn decode_frame(&mut self, data: &[u8], pts: i64) -> Result<Frame, String> {
        let width = self.width as usize;
        let height = self.height as usize;
        let stride = self.stride;
        let slice_height = self.slice_height;
        let y_size = stride.checked_mul(slice_height).ok_or("Bad luma size")?;
        let chroma_stride = stride.div_ceil(2);
        let chroma_rows = slice_height.div_ceil(2);
        let uv_size = chroma_stride.checked_mul(chroma_rows).ok_or("Bad chroma size")?;
        let planar = self.color_format == COLOR_YUV420_PLANAR;
        let semi_planar = matches!(
            self.color_format,
            COLOR_YUV420_SEMIPLANAR | COLOR_YUV420_PACKED_SEMIPLANAR |
            COLOR_YUV420_FLEXIBLE | COLOR_QCOM_NV12 |
            COLOR_QCOM_NV12_ALT | COLOR_QCOM_NV12_VENUS
        );
        if !planar && !semi_planar {
            return Err(format!("Unsupported MediaCodec YUV color format {:#x}", self.color_format));
        }
        let required = if planar { y_size + uv_size * 2 } else { y_size + stride * chroma_rows };
        if data.len() < required {
            return Err(format!(
                "Truncated decoder YUV buffer: {} bytes, expected {required}", data.len()
            ));
        }
        let mut pixels = vec![0u8; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let luma = i32::from(data[y * stride + x]);
                let chroma_row = y / 2;
                let chroma_col = x / 2;
                let (u, v) = if planar {
                    (
                        i32::from(data[y_size + chroma_row * chroma_stride + chroma_col]),
                        i32::from(data[y_size + uv_size + chroma_row * chroma_stride + chroma_col]),
                    )
                } else {
                    let uv = y_size + chroma_row * stride + chroma_col * 2;
                    (i32::from(data[uv]), i32::from(data[uv + 1]))
                };
                let c = (luma - 16).max(0);
                let d = u - 128;
                let e = v - 128;
                let offset = (y * width + x) * 4;
                pixels[offset] = ((298 * c + 409 * e + 128) >> 8).clamp(0, 255) as u8;
                pixels[offset + 1] =
                    ((298 * c - 100 * d - 208 * e + 128) >> 8).clamp(0, 255) as u8;
                pixels[offset + 2] = ((298 * c + 516 * d + 128) >> 8).clamp(0, 255) as u8;
                pixels[offset + 3] = 255;
            }
        }
        Ok(Frame { pixels, width: self.width, height: self.height, time_us: pts })
    }

    pub fn frame_for_time(&mut self, target_us: i64) -> Result<Option<Frame>, String> {
        // Preserve the final pending frame even after the codec emits EOS.
        // Its presentation timestamp can lie beyond the previous poll.
        if self.pending.as_ref().is_some_and(|f| f.time_us > target_us) {
            return Ok(None);
        }
        let mut display = self.pending.take();
        if self.output_finished { return Ok(display); }
        for _ in 0..16 {
            // Keep codec input supplied without blocking the emulation loop.
            let _ = self.feed_sample()?;
            let mut info = BufferInfo::default();
            let index = unsafe { AMediaCodec_dequeueOutputBuffer(self.codec, &mut info, 0) };
            if index == OUTPUT_FORMAT_CHANGED {
                self.refresh_output_format();
                continue;
            }
            if index == TRY_AGAIN_LATER { break; }
            if index < 0 { continue; }
            let mut buffer_capacity = 0usize;
            let ptr = unsafe {
                AMediaCodec_getOutputBuffer(self.codec, index as usize, &mut buffer_capacity)
            };
            // Always release the dequeued buffer, even if YUV conversion fails.
            let frame = if info.size > 0 && !ptr.is_null() {
                let offset = info.offset.max(0) as usize;
                let size = info.size as usize;
                if offset.checked_add(size).is_some_and(|end| end <= buffer_capacity) {
                    let bytes = unsafe { std::slice::from_raw_parts(ptr.add(offset), size) };
                    self.decode_frame(bytes, info.presentation_time_us).map(Some)
                } else {
                    Err("Invalid MediaCodec output buffer bounds".into())
                }
            } else { Ok(None) };
            unsafe { AMediaCodec_releaseOutputBuffer(self.codec, index as usize, false); }
            let frame = frame?;
            if info.flags & BUFFER_FLAG_END_OF_STREAM != 0 {
                self.output_finished = true;
            }
            if let Some(frame) = frame {
                if !self.reported_first_frame {
                    log!("Android video decoded first frame at {}us ({}x{})",
                        frame.time_us, frame.width, frame.height);
                    self.reported_first_frame = true;
                }
                if frame.time_us > target_us {
                    self.pending = Some(frame);
                    break;
                }
                display = Some(frame);
            }
            if self.output_finished { break; }
        }
        Ok(display)
    }

    pub fn is_finished(&self) -> bool {
        self.output_finished && self.pending.is_none()
    }
}

/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `AudioQueue.h` (Audio Queue Services)
//!
//! The audio playback here is mapped onto OpenAL Soft for convenience.
//! Apple's implementation probably uses Core Audio instead.

use crate::abi::{CallFromHost, GuestFunction};
use crate::audio::decode_ima4;
use crate::audio::openal as al;
use crate::audio::openal::al_types::*;
use crate::audio::openal::{OpenAL, OpenALManager};
use crate::dyld::{export_c_func, FunctionExports};
use crate::frameworks::carbon_core::OSStatus;
use crate::frameworks::core_audio_types::{
    debug_fourcc, fourcc, kAudioFormatAppleIMA4, kAudioFormatFlagIsBigEndian,
    kAudioFormatFlagIsFloat, kAudioFormatFlagIsPacked, kAudioFormatFlagIsSignedInteger,
    kAudioFormatLinearPCM, AudioStreamBasicDescription, AudioTimeStamp,
};
use crate::frameworks::core_foundation::cf_run_loop::{
    kCFRunLoopCommonModes, CFRunLoopGetMain, CFRunLoopMode, CFRunLoopRef,
};
use crate::frameworks::foundation::ns_run_loop;
use crate::frameworks::foundation::ns_string::get_static_str;
use crate::mem::{
    guest_size_of, ConstPtr, ConstVoidPtr, GuestUSize, Mem, MutPtr, MutVoidPtr, Ptr, SafeRead,
};
use crate::objc::msg;
use crate::Environment;
use std::collections::{HashMap, VecDeque};

#[derive(Default)]
pub struct State {
    audio_queues: HashMap<AudioQueueRef, AudioQueueHostObject>,
}
impl State {
    fn get(framework_state: &mut crate::frameworks::State) -> &mut Self {
        &mut framework_state.audio_toolbox.audio_queue
    }
    fn get_with_context<'s, 'm: 's>(
        framework_state: &'s mut crate::frameworks::State,
        manager: &'m mut OpenALManager,
    ) -> (&'s mut Self, OpenAL<'s>) {
        (
            &mut framework_state.audio_toolbox.audio_queue,
            framework_state
                .audio_toolbox
                .al_context
                .make_al_context_current(manager),
        )
    }
}

struct AudioQueueHostObject {
    format: AudioStreamBasicDescription,
    callback_proc: AudioQueueOutputCallback,
    callback_user_data: MutVoidPtr,
    /// Weak reference
    run_loop: CFRunLoopRef,
    volume: f32,
    buffers: Vec<AudioQueueBufferRef>,
    /// There is also a queue of OpenAL buffers, which must be kept in sync:
    /// the nth item in this queue must also be the nth item in the OpenAL
    /// queue, though the OpenAL queue may be shorter.
    buffer_queue: VecDeque<AudioQueueBufferRef>,
    is_running: AudioQueueIsRunning,
    al_source: Option<ALuint>,
    al_unused_buffers: Vec<ALuint>,
    aq_is_running_proc: Option<AudioQueuePropertyListenerProc>,
    aq_is_running_user_data: Option<MutVoidPtr>,
    is_running_handler: bool,
    /// Output format while the queue is in offline-rendering mode.
    offline_render_format: Option<AudioStreamBasicDescription>,
    /// Byte offset into the front queued input buffer during offline rendering.
    offline_buffer_offset: GuestUSize,
}

/// Track whether the audio queue is meant to be running, in order to handle
/// OpenAL stop events caused by running out of data:
/// - If it's running, the OpenAL source can be restarted.
/// - If it's stopping asynchronously, the audio queue stop can be completed.
#[derive(PartialEq, Eq, Clone, Copy)]
enum AudioQueueIsRunning {
    Running,
    Stopping,
    Stopped,
}

#[repr(C, packed)]
pub struct OpaqueAudioQueue {
    _filler: u8,
}
unsafe impl SafeRead for OpaqueAudioQueue {}

pub type AudioQueueRef = MutPtr<OpaqueAudioQueue>;

#[repr(C, packed)]
pub struct AudioQueueBuffer {
    audio_data_bytes_capacity: u32,
    pub audio_data: MutVoidPtr,
    pub audio_data_byte_size: u32,
    user_data: MutVoidPtr,
    packet_description_capacity: u32,
    /// Should be a `MutPtr<AudioStreamPacketDescription>`, but that's not
    /// implemented yet.
    _packet_descriptions: MutVoidPtr,
    _packet_description_count: u32,
}
unsafe impl SafeRead for AudioQueueBuffer {}

pub type AudioQueueBufferRef = MutPtr<AudioQueueBuffer>;

/// (*void)(void *in_user_data, AudioQueueRef in_aq, AudioQueueBufferRef in_buf)
pub type AudioQueueOutputCallback = GuestFunction;

type AudioQueueParameterID = u32;
pub const kAudioQueueParam_Volume: AudioQueueParameterID = 1;

type AudioQueueParameterValue = f32;

pub type AudioQueuePropertyID = u32;
pub const kAudioQueueProperty_IsRunning: AudioQueuePropertyID = fourcc(b"aqrn");

/// (*void)(void *in_user_data, AudioQueueRef in_aq, AudioQueuePropertyID in_id)
type AudioQueuePropertyListenerProc = GuestFunction;

const kAudioQueueErr_InvalidBuffer: OSStatus = -66687;
const kAudioQueueErr_InvalidPropertySize: OSStatus = -66683;
const kAudioQueueErr_CannotStart: OSStatus = -66681;
const kAudioQueueErr_InvalidDevice: OSStatus = -66680;
const kAudioQueueErr_BufferInQueue: OSStatus = -66679;

pub fn AudioQueueNewOutput(
    env: &mut Environment,
    in_format: ConstPtr<AudioStreamBasicDescription>,
    in_callback_proc: AudioQueueOutputCallback,
    in_user_data: MutVoidPtr,
    in_callback_run_loop: CFRunLoopRef,
    in_callback_run_loop_mode: CFRunLoopMode,
    in_flags: u32,
    out_aq: MutPtr<AudioQueueRef>,
) -> OSStatus {
    // reserved
    assert!(in_flags == 0);
    // NULL is a synonym of kCFRunLoopCommonModes here
    assert!(
        in_callback_run_loop_mode.is_null() || {
            let common_modes = get_static_str(env, kCFRunLoopCommonModes);
            msg![env; in_callback_run_loop_mode isEqual:common_modes]
        }
    );

    let in_callback_run_loop = if in_callback_run_loop.is_null() {
        // FIXME: According to the documentation, "one of the audio queue's
        // internal threads" should be used if you don't specify a run loop.
        // We should have an "internal thread" instead of using the main thread.
        CFRunLoopGetMain(env)
    } else {
        in_callback_run_loop
    };

    let mut format = env.mem.read(in_format);
    if env.bundle.bundle_identifier().starts_with("com.ea.candcra")
        && format.format_id == fourcc(b".mp3")
    {
        log!("Applying game-specific hack for C&C Red Alert: Fixing hardcoded audio format from .mp3 to PCM.");
        format = AudioStreamBasicDescription {
            sample_rate: 44100.0,
            format_id: kAudioFormatLinearPCM,
            format_flags: 12,
            bytes_per_packet: 4,
            frames_per_packet: 1,
            bytes_per_frame: 4,
            channels_per_frame: 2,
            bits_per_channel: 16,
            _reserved: 0,
        }
    }

    let host_object = AudioQueueHostObject {
        format,
        callback_proc: in_callback_proc,
        callback_user_data: in_user_data,
        run_loop: in_callback_run_loop,
        volume: 1.0,
        buffers: Vec::new(),
        buffer_queue: VecDeque::new(),
        is_running: AudioQueueIsRunning::Stopped,
        al_source: None,
        al_unused_buffers: Vec::new(),
        aq_is_running_proc: None,
        aq_is_running_user_data: None,
        is_running_handler: false,
        offline_render_format: None,
        offline_buffer_offset: 0,
    };

    let aq_ref = env.mem.alloc_and_write(OpaqueAudioQueue { _filler: 0 });
    State::get(&mut env.framework_state)
        .audio_queues
        .insert(aq_ref, host_object);
    env.mem.write(out_aq, aq_ref);

    ns_run_loop::add_audio_queue(env, in_callback_run_loop, aq_ref);

    log_if_broken_audio_format(&format);

    if !is_supported_audio_format(&format) {
        log_dbg!("Warning: Audio queue {:?} will be ignored because its format is not yet supported: {:#?}", aq_ref, format);
    }

    log_dbg!(
        "AudioQueueNewOutput() for format {:#?}, new audio queue handle: {:?}",
        format,
        aq_ref,
    );

    0 // success
}

pub fn AudioQueueGetParameter(
    env: &mut Environment,
    in_aq: AudioQueueRef,
    in_param_id: AudioQueueParameterID,
    out_value: MutPtr<AudioQueueParameterValue>,
) -> OSStatus {
    return_if_null!(in_aq);

    assert!(in_param_id == kAudioQueueParam_Volume); // others unimplemented

    let state = State::get(&mut env.framework_state);
    let host_object = state.audio_queues.get_mut(&in_aq).unwrap();

    env.mem.write(out_value, host_object.volume);

    0 // success
}

pub fn AudioQueueSetParameter(
    env: &mut Environment,
    in_aq: AudioQueueRef,
    in_param_id: AudioQueueParameterID,
    in_value: AudioQueueParameterValue,
) -> OSStatus {
    return_if_null!(in_aq);

    assert!(in_param_id == kAudioQueueParam_Volume); // others unimplemented

    let state = State::get(&mut env.framework_state);
    let host_object = state.audio_queues.get_mut(&in_aq).unwrap();

    host_object.volume = in_value;
    log_dbg!(
        "AudioQueueSetParameter kAudioQueueParam_Volume is set to {}",
        host_object.volume
    );
    if let Some(al_source) = host_object.al_source {
        let context = env
            .framework_state
            .audio_toolbox
            .make_al_context_current(&mut env.openal_manager);

        // If not clamped, OpenAL generates an error.
        // While Apple's docs states that this range is expected,
        // setting outside of range values do not generate errors
        // (tested on both macOS and iOS).
        let in_value = in_value.clamp(0.0, 1.0);

        unsafe {
            context.Sourcef(al_source, al::AL_MAX_GAIN, in_value);
            assert!(context.GetError() == 0);
        }
    }

    0 // success
}

fn AudioQueueAllocateBufferWithPacketDescriptions(
    env: &mut Environment,
    in_aq: AudioQueueRef,
    in_buffer_byte_size: GuestUSize,
    _in_number_packet_desc: GuestUSize,
    out_buffer: MutPtr<AudioQueueBufferRef>,
) -> OSStatus {
    // TODO: support packet descriptions
    AudioQueueAllocateBuffer(env, in_aq, in_buffer_byte_size, out_buffer)
}

pub fn AudioQueueAllocateBuffer(
    env: &mut Environment,
    in_aq: AudioQueueRef,
    in_buffer_byte_size: GuestUSize,
    out_buffer: MutPtr<AudioQueueBufferRef>,
) -> OSStatus {
    return_if_null!(in_aq);

    let host_object = State::get(&mut env.framework_state)
        .audio_queues
        .get_mut(&in_aq)
        .unwrap();

    let packet_description_capacity = if env
        .bundle
        .bundle_identifier()
        .starts_with("com.ea.candcra")
    {
        log!("Applying game-specific hack for C&C Red Alert: Setting packet description capacity to 1024.");
        1024
    } else {
        0
    };

    let audio_data = env.mem.alloc(in_buffer_byte_size);
    let buffer_ptr = env.mem.alloc_and_write(AudioQueueBuffer {
        audio_data_bytes_capacity: in_buffer_byte_size,
        audio_data,
        audio_data_byte_size: 0,
        user_data: Ptr::null(),
        packet_description_capacity,
        _packet_descriptions: Ptr::null(),
        _packet_description_count: 0,
    });
    host_object.buffers.push(buffer_ptr);
    env.mem.write(out_buffer, buffer_ptr);

    0 // success
}

fn AudioQueueEnqueueBufferWithParameters(
    env: &mut Environment,
    in_aq: AudioQueueRef,
    in_buffer: AudioQueueBufferRef,
    in_num_packet_descs: u32,
    in_packet_descs: MutVoidPtr,
    in_trim_frames_at_start: u32,
    in_trim_frames_at_end: u32,
    in_num_param_values: u32,
    in_param_values: MutVoidPtr,
    in_start_time: ConstPtr<AudioTimeStamp>,
    out_actual_start_time: MutPtr<AudioTimeStamp>,
) -> OSStatus {
    // TODO
    assert_eq!(in_trim_frames_at_start, 0);
    assert_eq!(in_trim_frames_at_end, 0);
    assert_eq!(in_num_param_values, 0);
    assert!(in_param_values.is_null());
    assert!(in_start_time.is_null());
    assert!(out_actual_start_time.is_null());
    AudioQueueEnqueueBuffer(env, in_aq, in_buffer, in_num_packet_descs, in_packet_descs)
}

pub fn AudioQueueEnqueueBuffer(
    env: &mut Environment,
    in_aq: AudioQueueRef,
    in_buffer: AudioQueueBufferRef,
    _in_num_packet_descs: u32,
    _in_packet_descs: MutVoidPtr,
) -> OSStatus {
    return_if_null!(in_aq);

    // Variable packet size unimplemented (no formats supported that need it).
    // We don't assert the count is 0 because we might get a useless one even
    // for formats that don't need it.

    let host_object = State::get(&mut env.framework_state)
        .audio_queues
        .get_mut(&in_aq)
        .unwrap();

    if !host_object.buffers.contains(&in_buffer) {
        return kAudioQueueErr_InvalidBuffer;
    }

    host_object.buffer_queue.push_back(in_buffer);
    log_dbg!("New buffer enqueued: {:?}", in_buffer);

    0 // success
}

fn AudioQueueAddPropertyListener(
    env: &mut Environment,
    in_aq: AudioQueueRef,
    in_id: AudioQueuePropertyID,
    in_proc: AudioQueuePropertyListenerProc,
    in_user_data: MutVoidPtr,
) -> OSStatus {
    return_if_null!(in_aq);

    if in_id == kAudioQueueProperty_IsRunning {
        let host_object = State::get(&mut env.framework_state)
            .audio_queues
            .get_mut(&in_aq)
            .unwrap();

        host_object.aq_is_running_proc = Some(in_proc);
        host_object.aq_is_running_user_data = Some(in_user_data);
    } else {
        log!(
            "TODO: AudioQueueAddPropertyListener({:?}, {}, {:?}, {:?})",
            in_aq,
            debug_fourcc(in_id),
            in_proc,
            in_user_data
        );
    }
    0 // success
}
fn AudioQueueRemovePropertyListener(
    env: &mut Environment,
    in_aq: AudioQueueRef,
    in_id: AudioQueuePropertyID,
    in_proc: AudioQueuePropertyListenerProc,
    in_user_data: MutVoidPtr,
) -> OSStatus {
    return_if_null!(in_aq);

    if in_id == kAudioQueueProperty_IsRunning {
        let host_object = State::get(&mut env.framework_state)
            .audio_queues
            .get_mut(&in_aq)
            .unwrap();

        host_object.aq_is_running_proc = None;
        host_object.aq_is_running_user_data = None;
    } else {
        log!(
            "TODO: AudioQueueRemovePropertyListener({:?}, {}, {:?}, {:?})",
            in_aq,
            debug_fourcc(in_id),
            in_proc,
            in_user_data
        );
    }
    0 // success
}

fn property_size(property_id: AudioQueuePropertyID) -> GuestUSize {
    match property_id {
        kAudioQueueProperty_IsRunning => guest_size_of::<u32>(),
        _ => unimplemented!("Unimplemented property ID: {}", debug_fourcc(property_id)),
    }
}

fn AudioQueueGetPropertySize(
    env: &mut Environment,
    in_aq: AudioQueueRef,
    in_property_id: AudioQueuePropertyID,
    out_data_size: MutPtr<u32>,
) -> OSStatus {
    return_if_null!(in_aq);

    env.mem.write(out_data_size, property_size(in_property_id));
    0 // success
}

fn AudioQueueGetProperty(
    env: &mut Environment,
    in_aq: AudioQueueRef,
    in_property_id: AudioQueuePropertyID,
    out_property_data: MutVoidPtr,
    io_data_size: MutPtr<u32>,
) -> OSStatus {
    return_if_null!(in_aq);

    let required_size = property_size(in_property_id);
    if env.mem.read(io_data_size) != required_size {
        log!("Warning: AudioQueueGetProperty() failed");
        return kAudioQueueErr_InvalidPropertySize;
    }

    let host_object = State::get(&mut env.framework_state)
        .audio_queues
        .get_mut(&in_aq)
        .unwrap();

    match in_property_id {
        kAudioQueueProperty_IsRunning => {
            let is_running: u32 = match host_object.is_running {
                AudioQueueIsRunning::Running => 1,
                AudioQueueIsRunning::Stopping => 1,
                AudioQueueIsRunning::Stopped => 0,
            };
            env.mem.write(out_property_data.cast(), is_running);
        }
        _ => unreachable!(),
    }

    0 // success
}

fn AudioQueueSetProperty(
    _env: &mut Environment,
    in_aq: AudioQueueRef,
    in_property_id: AudioQueuePropertyID,
    in_property_data: ConstVoidPtr,
    io_data_size: MutPtr<u32>,
) -> OSStatus {
    log!(
        "TODO: AudioQueueSetProperty({:?}, {}, {:?}, {:?}) -> kAudioQueueErr_InvalidDevice",
        in_aq,
        debug_fourcc(in_property_id),
        in_property_data,
        io_data_size
    );

    // Error value shouldn't matter that much,
    // this one is closest to a notion of "unsupported"
    kAudioQueueErr_InvalidDevice
}

fn AudioQueueSetOfflineRenderFormat(
    env: &mut Environment,
    in_aq: AudioQueueRef,
    in_format: ConstPtr<AudioStreamBasicDescription>,
    in_layout: ConstVoidPtr,
) -> OSStatus {
    return_if_null!(in_aq);

    if in_format.is_null() {
        if !in_layout.is_null() {
            log!("Warning: ignoring channel layout while disabling offline rendering");
        }
        let host_object = State::get(&mut env.framework_state)
            .audio_queues
            .get_mut(&in_aq)
            .unwrap();
        host_object.offline_render_format = None;
        host_object.offline_buffer_offset = 0;
        return 0;
    }

    let format = env.mem.read(in_format);
    if format.format_id != kAudioFormatLinearPCM || !is_supported_audio_format(&format) {
        log!(
            "Warning: unsupported AudioQueue offline render format: {:?}",
            format
        );
        return kAudioQueueErr_InvalidDevice;
    }

    if !in_layout.is_null() {
        log_once!("Ignoring AudioQueue offline render channel layout");
    }

    let host_object = State::get(&mut env.framework_state)
        .audio_queues
        .get_mut(&in_aq)
        .unwrap();
    host_object.offline_render_format = Some(format);
    host_object.offline_buffer_offset = 0;

    log_dbg!(
        "AudioQueueSetOfflineRenderFormat({:?}) -> {:?}",
        in_aq,
        format
    );
    0
}

fn AudioQueueOfflineRender(
    env: &mut Environment,
    in_aq: AudioQueueRef,
    _in_timestamp: ConstPtr<AudioTimeStamp>,
    io_buffer: AudioQueueBufferRef,
    in_number_frames: u32,
) -> OSStatus {
    return_if_null!(in_aq);

    if io_buffer.is_null() {
        return kAudioQueueErr_InvalidBuffer;
    }

    let (source_format, output_format, output_is_owned, input_buffer_count) = {
        let host_object = State::get(&mut env.framework_state)
            .audio_queues
            .get_mut(&in_aq)
            .unwrap();
        (
            host_object.format,
            host_object.offline_render_format,
            host_object.buffers.contains(&io_buffer),
            host_object.buffers.len(),
        )
    };

    let Some(output_format) = output_format else {
        log!("Warning: AudioQueueOfflineRender called outside offline mode");
        return kAudioQueueErr_InvalidDevice;
    };

    // AudioFile currently exposes supported compressed containers as decoded
    // PCM. Supporting identical PCM formats here therefore provides a real
    // offline-rendering path without introducing a second audio converter.
    if source_format != output_format {
        log!(
            "Warning: AudioQueue offline conversion is not implemented: {:?} -> {:?}",
            source_format,
            output_format
        );
        return kAudioQueueErr_InvalidDevice;
    }

    if !output_is_owned {
        return kAudioQueueErr_InvalidBuffer;
    }

    let output = env.mem.read(io_buffer);
    let requested_bytes = in_number_frames
        .checked_mul(output_format.bytes_per_frame)
        .unwrap();
    let output_capacity = output.audio_data_bytes_capacity;
    if requested_bytes > output_capacity {
        log!(
            "Warning: AudioQueueOfflineRender requested {} bytes for a {} byte buffer",
            requested_bytes,
            output_capacity
        );
        return kAudioQueueErr_InvalidBuffer;
    }

    let mut output_size: GuestUSize = 0;
    let mut empty_callback_count = 0usize;

    while output_size < requested_bytes {
        let (input_buffer, input_offset, callback_proc, callback_user_data) = {
            let host_object = State::get(&mut env.framework_state)
                .audio_queues
                .get_mut(&in_aq)
                .unwrap();
            let Some(&input_buffer) = host_object.buffer_queue.front() else {
                break;
            };
            (
                input_buffer,
                host_object.offline_buffer_offset,
                host_object.callback_proc,
                host_object.callback_user_data,
            )
        };

        // The buffer supplied for rendered output must not simultaneously be
        // an enqueued source buffer.
        if input_buffer == io_buffer {
            return kAudioQueueErr_InvalidBuffer;
        }

        let input = env.mem.read(input_buffer);
        if input_offset >= input.audio_data_byte_size {
            {
                let host_object = State::get(&mut env.framework_state)
                    .audio_queues
                    .get_mut(&in_aq)
                    .unwrap();
                assert_eq!(host_object.buffer_queue.pop_front(), Some(input_buffer));
                host_object.offline_buffer_offset = 0;
            }

            let (): () =
                callback_proc.call_from_host(env, (callback_user_data, in_aq, input_buffer));

            if input.audio_data_byte_size == 0 {
                empty_callback_count += 1;
                if empty_callback_count > input_buffer_count {
                    break;
                }
            } else {
                empty_callback_count = 0;
            }
            continue;
        }

        let available = input.audio_data_byte_size - input_offset;
        let remaining = requested_bytes - output_size;
        let bytes_to_copy = available.min(remaining);

        let source = env
            .mem
            .bytes_at(input.audio_data.cast(), input.audio_data_byte_size)[input_offset as usize..]
            [..bytes_to_copy as usize]
            .to_vec();
        env.mem
            .bytes_at_mut(output.audio_data.cast(), requested_bytes)[output_size as usize..]
            [..bytes_to_copy as usize]
            .copy_from_slice(&source);

        output_size += bytes_to_copy;
        State::get(&mut env.framework_state)
            .audio_queues
            .get_mut(&in_aq)
            .unwrap()
            .offline_buffer_offset += bytes_to_copy;
    }

    let mut output = env.mem.read(io_buffer);
    output.audio_data_byte_size = output_size;
    env.mem.write(io_buffer, output);

    log_dbg!(
        "AudioQueueOfflineRender({:?}) rendered {} of {} requested frames",
        in_aq,
        output_size / output_format.bytes_per_frame,
        in_number_frames
    );
    0
}

pub fn log_if_broken_audio_format(format: &AudioStreamBasicDescription) {
    let bytes_per_channel = format.bits_per_channel / 8;
    let expected_bytes_per_packet = format.bytes_per_frame * format.frames_per_packet;
    let expected_bytes_per_frame = format.channels_per_frame * bytes_per_channel;
    if format.bytes_per_packet < expected_bytes_per_packet
        || format.bytes_per_frame < expected_bytes_per_frame
    {
        log!(
            "Warning: Stream format has non-sensical values: {:?}",
            format
        );
    }
}

/// Check if the format of an audio queue is one we currently support.
/// If not, we should skip trying to play it rather than crash.
pub fn is_supported_audio_format(format: &AudioStreamBasicDescription) -> bool {
    let &AudioStreamBasicDescription {
        format_id,
        format_flags,
        channels_per_frame,
        bits_per_channel,
        bytes_per_frame,
        ..
    } = format;
    match format_id {
        kAudioFormatAppleIMA4 => (channels_per_frame == 1) || (channels_per_frame == 2),
        kAudioFormatLinearPCM => {
            // TODO: support more PCM formats
            (channels_per_frame == 1 || channels_per_frame == 2)
                && (bits_per_channel == 8 || bits_per_channel == 16 || bits_per_channel == 32)
                && ((format_flags & kAudioFormatFlagIsPacked) != 0
                    || ((bits_per_channel / 8) * channels_per_frame) == bytes_per_frame)
                && (format_flags & kAudioFormatFlagIsBigEndian) == 0
                && (format_flags & kAudioFormatFlagIsFloat) == 0
        }
        _ => false,
    }
}

/// Decode an [AudioQueueBuffer] or [super::audio_unit::AudioBuffer]'s content
/// to raw PCM suitable for an OpenAL buffer.
pub fn decode_buffer(
    mem: &Mem,
    format: &AudioStreamBasicDescription,
    audio_data: MutPtr<u8>,
    audio_data_byte_size: GuestUSize,
) -> (ALenum, ALsizei, Vec<u8>) {
    let data_slice = mem.bytes_at(audio_data, audio_data_byte_size);

    assert!(is_supported_audio_format(format));

    match format.format_id {
        kAudioFormatAppleIMA4 => {
            assert!(data_slice.len().is_multiple_of(34));
            let mut out_pcm = Vec::<u8>::with_capacity((data_slice.len() / 34) * 64 * 2);
            let packets = data_slice.chunks(34);

            if format.channels_per_frame == 1 {
                for packet in packets {
                    let pcm_packet: [i16; 64] = decode_ima4(packet.try_into().unwrap());
                    let pcm_bytes: &[u8] = unsafe {
                        std::slice::from_raw_parts(pcm_packet.as_ptr() as *const u8, 128)
                    };
                    out_pcm.extend_from_slice(pcm_bytes);
                }

                (al::AL_FORMAT_MONO16, format.sample_rate as ALsizei, out_pcm)
            } else {
                let mut peekable_packets = packets.peekable();
                while peekable_packets.peek().is_some() {
                    let left = peekable_packets.next().unwrap();
                    let left_pcm_packet: [i16; 64] = decode_ima4(left.try_into().unwrap());
                    let right = peekable_packets.next().unwrap();
                    let right_pcm_packet: [i16; 64] = decode_ima4(right.try_into().unwrap());
                    for (l, r) in left_pcm_packet.iter().zip(right_pcm_packet.iter()) {
                        out_pcm.extend_from_slice(&l.to_le_bytes());
                        out_pcm.extend_from_slice(&r.to_le_bytes());
                    }
                }

                (
                    al::AL_FORMAT_STEREO16,
                    format.sample_rate as ALsizei,
                    out_pcm,
                )
            }
        }
        kAudioFormatLinearPCM => {
            // The end of the data might be misaligned (this happens in Crash
            // Bandicoot Nitro Kart 3D somehow).
            let misaligned_by = data_slice.len() % (format.bytes_per_frame as usize);
            let data_slice = if misaligned_by != 0 {
                &data_slice[..data_slice.len() - misaligned_by]
            } else {
                data_slice
            };

            let bytes_per_channel = format.bits_per_channel / 8;
            let actual_bytes_per_frame = format.channels_per_frame * bytes_per_channel;
            let actual_channels_per_frame = format.bytes_per_frame / bytes_per_channel;

            // In case the audio format has inconsistent values, we apply some
            // processing before passing it to OpenAL.
            // This is the case in Resident Evil 4
            let processed_data: Vec<u8> = if actual_bytes_per_frame == format.bytes_per_frame {
                data_slice.to_owned()
            } else {
                let actual_frame_count = data_slice.len() / actual_bytes_per_frame as usize;
                let processed_frame_count = format.bytes_per_frame as usize * actual_frame_count;
                let mut processed_data = Vec::<u8>::with_capacity(processed_frame_count);
                for frame in data_slice.chunks(actual_bytes_per_frame as usize) {
                    // Fetch only frame bytes
                    let frame_bytes = &frame[frame.len() - format.bytes_per_frame as usize..];
                    // Change from big to little endian
                    // It's been observed in Resident Evil 4 that, although the
                    // audio format doesn't say anything about it being in big
                    // endian, the data in the buffer has their values in big
                    // endian and must be converted to little endian before
                    // passing them to OpenAL.
                    match format.bytes_per_frame {
                        1 => processed_data.extend(
                            &u8::from_be_bytes(frame_bytes.try_into().unwrap()).to_le_bytes(),
                        ),
                        2 => processed_data.extend_from_slice(
                            &u16::from_be_bytes(frame_bytes.try_into().unwrap()).to_le_bytes(),
                        ),
                        4 => processed_data.extend_from_slice(
                            &u32::from_be_bytes(frame_bytes.try_into().unwrap()).to_le_bytes(),
                        ),
                        8 => processed_data.extend_from_slice(
                            &u64::from_be_bytes(frame_bytes.try_into().unwrap()).to_le_bytes(),
                        ),
                        16 => processed_data.extend_from_slice(
                            &u128::from_be_bytes(frame_bytes.try_into().unwrap()).to_le_bytes(),
                        ),
                        _ => unimplemented!(),
                    };
                }
                processed_data
            };

            let f = match (actual_channels_per_frame, format.bits_per_channel) {
                (1, 8) => al::AL_FORMAT_MONO8,
                (1, 16) => al::AL_FORMAT_MONO16,
                (2, 8) => al::AL_FORMAT_STEREO8,
                (2, 16) => al::AL_FORMAT_STEREO16,
                (2, 32) => {
                    assert!((format.format_flags & kAudioFormatFlagIsSignedInteger) != 0);
                    assert!(processed_data.len().is_multiple_of(4));
                    let new_size = (processed_data.len() / 4) * 2; // size from 32-bit to 16-bit
                    let mut new_processed_data = Vec::<u8>::with_capacity(new_size);
                    for chunk in processed_data.chunks(4) {
                        let val: i32 = i32::from_le_bytes(chunk.try_into().unwrap());
                        let new_val: i16 = (val >> 16) as i16;
                        new_processed_data.extend(new_val.to_le_bytes());
                    }
                    return (
                        al::AL_FORMAT_STEREO16,
                        format.sample_rate as ALsizei,
                        new_processed_data,
                    );
                }
                _ => unreachable!(),
            };
            (f, format.sample_rate as ALsizei, processed_data)
        }
        _ => unreachable!(),
    }
}

/// Ensure an audio queue has an OpenAL source and at least one queued OpenAL
/// buffer.
fn prime_audio_queue(env: &mut Environment, in_aq: AudioQueueRef) -> Result<(), ()> {
    let (state, context) =
        State::get_with_context(&mut env.framework_state, &mut env.openal_manager);

    let host_object = state.audio_queues.get_mut(&in_aq).unwrap();

    if !is_supported_audio_format(&host_object.format) {
        return Err(());
    }

    if host_object.al_source.is_none() {
        // If not clamped, OpenAL generates an error.
        // While Apple's docs states that this range is expected,
        // setting outside of range values do not generate errors
        // (tested on both macOS and iOS).
        let volume = host_object.volume.clamp(0.0, 1.0);
        let mut al_source = 0;
        unsafe {
            context.GenSources(1, &mut al_source);
            context.Sourcef(al_source, al::AL_MAX_GAIN, volume);
            assert!(context.GetError() == 0);
        };
        host_object.al_source = Some(al_source);
    }
    let al_source = host_object.al_source.unwrap();

    loop {
        let mut al_buffers_queued = 0;
        let mut al_buffers_processed = 0;
        unsafe {
            context.GetSourcei(al_source, al::AL_BUFFERS_QUEUED, &mut al_buffers_queued);
            context.GetSourcei(
                al_source,
                al::AL_BUFFERS_PROCESSED,
                &mut al_buffers_processed,
            );
            assert!(context.GetError() == 0);
        }
        let al_buffers_queued: usize = al_buffers_queued.try_into().unwrap();
        let al_buffers_processed: usize = al_buffers_processed.try_into().unwrap();

        assert!(al_buffers_queued <= host_object.buffer_queue.len());
        let unprocessed_buffers = al_buffers_queued - al_buffers_processed;

        if unprocessed_buffers > 1 || al_buffers_queued == host_object.buffer_queue.len() {
            break;
        }

        let next_buffer_idx = al_buffers_queued;
        let next_buffer_ref = host_object.buffer_queue[next_buffer_idx];
        let next_buffer = env.mem.read(next_buffer_ref);

        log_dbg!(
            "Decoding buffer {:?} for queue {:?}",
            next_buffer_ref,
            in_aq
        );

        let next_al_buffer = host_object.al_unused_buffers.pop().unwrap_or_else(|| {
            let mut al_buffer = 0;
            unsafe { context.GenBuffers(1, &mut al_buffer) };
            assert!(unsafe { context.GetError() } == 0);
            al_buffer
        });

        let (al_format, al_frequency, data) = decode_buffer(
            &env.mem,
            &host_object.format,
            next_buffer.audio_data.cast(),
            next_buffer.audio_data_byte_size,
        );
        unsafe {
            context.BufferData(
                next_al_buffer,
                al_format,
                data.as_ptr() as *const ALvoid,
                data.len().try_into().unwrap(),
                al_frequency,
            )
        };
        unsafe { context.SourceQueueBuffers(al_source, 1, &next_al_buffer) };
        assert!(unsafe { context.GetError() } == 0);
    }
    Ok(())
}

fn unqueue_buffers<F: FnMut(ALuint)>(al_source: ALuint, context: &OpenAL<'_>, mut callback: F) {
    loop {
        let mut al_buffers_processed = 0;
        unsafe {
            context.GetSourcei(
                al_source,
                al::AL_BUFFERS_PROCESSED,
                &mut al_buffers_processed,
            );
            assert!(context.GetError() == 0);
        }
        if al_buffers_processed == 0 {
            break;
        }

        let mut al_buffer = 0;
        unsafe {
            context.SourceUnqueueBuffers(al_source, 1, &mut al_buffer);
            assert!(context.GetError() == 0);
        }

        callback(al_buffer);
    }
}

/// For use by `NSRunLoop`: check the status of an audio queue, recycle buffers,
/// call callbacks, push new buffers etc.
pub fn handle_audio_queue(env: &mut Environment, in_aq: AudioQueueRef) {
    // Collect used buffers and call the user callback so the app can provide
    // new buffers.

    let (state, context) =
        State::get_with_context(&mut env.framework_state, &mut env.openal_manager);

    let host_object = state.audio_queues.get_mut(&in_aq).unwrap();
    let Some(al_source) = host_object.al_source else {
        return;
    };
    if !is_supported_audio_format(&host_object.format) {
        return;
    }
    if host_object.is_running_handler {
        // Already running, prevent infinite loop from reentrancy
        return;
    }

    host_object.is_running_handler = true;

    let mut buffers_to_reuse = Vec::new();

    unqueue_buffers(al_source, &context, |al_buffer| {
        host_object.al_unused_buffers.push(al_buffer);
        let buffer_ref = host_object.buffer_queue.pop_front().unwrap();
        buffers_to_reuse.push(buffer_ref);
    });

    let &mut AudioQueueHostObject {
        callback_proc,
        callback_user_data,
        is_running,
        ..
    } = host_object;

    for buffer_ref in buffers_to_reuse.drain(..) {
        log_dbg!(
            "Recyling buffer {:?} for queue {:?}. Calling callback {:?} with user data {:?}.",
            buffer_ref,
            in_aq,
            callback_proc,
            callback_user_data
        );

        let () = callback_proc.call_from_host(env, (callback_user_data, in_aq, buffer_ref));
    }

    // Push new buffers etc.

    _ = prime_audio_queue(env, in_aq);

    let context = env
        .framework_state
        .audio_toolbox
        .make_al_context_current(&mut env.openal_manager);

    if is_running != AudioQueueIsRunning::Stopped {
        unsafe {
            let mut al_source_state = 0;
            context.GetSourcei(al_source, al::AL_SOURCE_STATE, &mut al_source_state);
            assert!(context.GetError() == 0);
            // Source probably ran out data and needs restarting
            // TODO: We currently have to do this even when touchHLE is not
            // lagging, because we're not ensuring OpenAL always has at least
            // one buffer it hasn't processed yet. We need to change our queue
            // handling.
            if al_source_state == al::AL_STOPPED {
                context.SourcePlay(al_source);
                log_dbg!("Restarted OpenAL source for queue {:?}", in_aq);
            }
        }
    }

    if is_running == AudioQueueIsRunning::Stopping {
        let mut al_source_state = 0;
        unsafe {
            context.GetSourcei(al_source, al::AL_SOURCE_STATE, &mut al_source_state);
            assert!(context.GetError() == 0);
        }

        // If OpenAL still says the source is stopped, it must have run out of
        // data, and therefore it's time to complete the "asynchronous stop".
        if al_source_state == al::AL_STOPPED {
            log_dbg!(
                "OpenAL source stopped for queue {:?}, completing asynchronous stop.",
                in_aq
            );
            finish_stopping_audio_queue(env, in_aq);
        }
    }

    if let Some(host_object) = State::get(&mut env.framework_state)
        .audio_queues
        .get_mut(&in_aq)
    {
        host_object.is_running_handler = false;
    }
}

fn AudioQueuePrime(
    env: &mut Environment,
    in_aq: AudioQueueRef,
    _in_number_of_frames_to_prepare: u32,
    out_number_of_frames_prepared: MutPtr<u32>,
) -> OSStatus {
    return_if_null!(in_aq);

    match prime_audio_queue(env, in_aq) {
        Ok(_) => {
            assert!(out_number_of_frames_prepared.is_null()); // TODO
            0 // success
        }
        Err(_) => {
            log!("Warning: Cannot prime audio queue!");
            kAudioQueueErr_CannotStart
        }
    }
}

fn notify_aq_is_running(env: &mut Environment, in_aq: AudioQueueRef) {
    let host_object = State::get(&mut env.framework_state)
        .audio_queues
        .get_mut(&in_aq)
        .unwrap();

    if let (Some(in_proc), Some(in_user_data)) = (
        host_object.aq_is_running_proc,
        host_object.aq_is_running_user_data,
    ) {
        <GuestFunction as CallFromHost<(), (MutVoidPtr, Ptr<OpaqueAudioQueue, true>, u32)>>::
        call_from_host(
            &in_proc, env, (in_user_data, in_aq, kAudioQueueProperty_IsRunning)
        );
    }
}

pub fn AudioQueueStart(
    env: &mut Environment,
    in_aq: AudioQueueRef,
    in_device_start_time: ConstPtr<AudioTimeStamp>,
) -> OSStatus {
    return_if_null!(in_aq);

    assert!(in_device_start_time.is_null()); // TODO

    let is_offline = State::get(&mut env.framework_state)
        .audio_queues
        .get_mut(&in_aq)
        .unwrap()
        .offline_render_format
        .is_some();

    if is_offline {
        State::get(&mut env.framework_state)
            .audio_queues
            .get_mut(&in_aq)
            .unwrap()
            .is_running = AudioQueueIsRunning::Running;
        notify_aq_is_running(env, in_aq);
        return 0;
    }

    _ = prime_audio_queue(env, in_aq);

    let (state, context) =
        State::get_with_context(&mut env.framework_state, &mut env.openal_manager);

    let host_object = state.audio_queues.get_mut(&in_aq).unwrap();

    host_object.is_running = AudioQueueIsRunning::Running;

    if is_supported_audio_format(&host_object.format) {
        let al_source = host_object.al_source.unwrap();
        unsafe { context.SourcePlay(al_source) };
        assert!(unsafe { context.GetError() } == 0);
    } else {
        log!(
            "AudioQueueStart: Unsupported format {:?}",
            host_object.format
        );
    }

    notify_aq_is_running(env, in_aq);

    0 // success
}

pub fn AudioQueuePause(env: &mut Environment, in_aq: AudioQueueRef) -> OSStatus {
    return_if_null!(in_aq);

    let (state, context) =
        State::get_with_context(&mut env.framework_state, &mut env.openal_manager);

    let host_object = state.audio_queues.get_mut(&in_aq).unwrap();
    // FIXME: is this correct? is it notifiable?
    host_object.is_running = AudioQueueIsRunning::Stopped;
    if let Some(al_source) = host_object.al_source {
        unsafe { context.SourcePause(al_source) };
        assert!(unsafe { context.GetError() } == 0);
    }

    0 // success
}

fn finish_stopping_audio_queue(env: &mut Environment, in_aq: AudioQueueRef) {
    // OpenAL stop is not done here because it would be redundant in the case
    // of an asynchronous stop, where the audio queue stopping is triggered by
    // the OpenAL queue stopping.
    AudioQueueReset(env, in_aq);
    State::get(&mut env.framework_state)
        .audio_queues
        .get_mut(&in_aq)
        .unwrap()
        .is_running = AudioQueueIsRunning::Stopped;
    notify_aq_is_running(env, in_aq);
}

pub fn AudioQueueStop(env: &mut Environment, in_aq: AudioQueueRef, in_immediate: bool) -> OSStatus {
    return_if_null!(in_aq);

    if in_immediate {
        log_dbg!("Performing immediate AudioQueueStop for {:?}.", in_aq);

        let (state, context) =
            State::get_with_context(&mut env.framework_state, &mut env.openal_manager);

        let host_object = state.audio_queues.get_mut(&in_aq).unwrap();
        if let Some(al_source) = host_object.al_source {
            unsafe { context.SourceStop(al_source) };
            assert!(unsafe { context.GetError() } == 0);
        };

        finish_stopping_audio_queue(env, in_aq);
    } else {
        let state = State::get(&mut env.framework_state);
        let host_object = state.audio_queues.get_mut(&in_aq).unwrap();
        if host_object.is_running != AudioQueueIsRunning::Stopped {
            log_dbg!("Starting asynchronous AudioQueueStop for {:?}.", in_aq);
            host_object.is_running = AudioQueueIsRunning::Stopping;
        } else {
            log_dbg!(
                "Ignoring asynchronous AudioQueueStop for {:?} (already stopped).",
                in_aq
            );
        }
    }

    0 // success
}

fn AudioQueueReset(env: &mut Environment, in_aq: AudioQueueRef) -> OSStatus {
    return_if_null!(in_aq);

    let (state, context) =
        State::get_with_context(&mut env.framework_state, &mut env.openal_manager);

    log_dbg!("Resetting queue {:?}.", in_aq);

    let host_object = state.audio_queues.get_mut(&in_aq).unwrap();

    if let Some(al_source) = host_object.al_source {
        unsafe {
            let mut al_source_state = 0;
            context.GetSourcei(al_source, al::AL_SOURCE_STATE, &mut al_source_state);
            assert!(context.GetError() == 0);
            if al_source_state != al::AL_STOPPED {
                // If the source is not already stopped, it must be stopped in
                // order to be able to clear its buffer queue. Note that the
                // audio queue may still be considered "running".
                context.SourceStop(al_source);
                assert!(context.GetError() == 0);
            }
        }

        unqueue_buffers(al_source, &context, |al_buffer| {
            host_object.al_unused_buffers.push(al_buffer);
            host_object.buffer_queue.pop_front().unwrap();
        });
    }

    host_object.buffer_queue.clear();
    host_object.offline_buffer_offset = 0;

    0 // success
}

fn AudioQueueFlush(_env: &mut Environment, in_aq: AudioQueueRef) -> OSStatus {
    return_if_null!(in_aq);
    // TODO
    0 // success
}

fn AudioQueueFreeBuffer(
    env: &mut Environment,
    in_aq: AudioQueueRef,
    in_buffer: AudioQueueBufferRef,
) -> OSStatus {
    return_if_null!(in_aq);

    let host_object = State::get(&mut env.framework_state)
        .audio_queues
        .get_mut(&in_aq)
        .unwrap();

    if host_object.buffer_queue.contains(&in_buffer) {
        return kAudioQueueErr_BufferInQueue;
    }

    if let Some(index) = host_object.buffers.iter().position(|x| x == &in_buffer) {
        host_object.buffers.remove(index);

        log_dbg!("Freeing buffer: {:?}", in_buffer);

        let buffer = env.mem.read(in_buffer);
        env.mem.free(buffer.audio_data);
        env.mem.free(in_buffer.cast());

        0 // success
    } else {
        kAudioQueueErr_InvalidBuffer
    }
}

pub fn AudioQueueDispose(
    env: &mut Environment,
    in_aq: AudioQueueRef,
    in_immediate: bool,
) -> OSStatus {
    return_if_null!(in_aq);

    assert!(in_immediate); // TODO

    let (state, context) =
        State::get_with_context(&mut env.framework_state, &mut env.openal_manager);

    let mut host_object = state.audio_queues.remove(&in_aq).unwrap();
    log_dbg!("Disposing of audio queue {:?}", in_aq);

    env.mem.free(in_aq.cast());

    for buffer_ptr in host_object.buffers {
        let buffer = env.mem.read(buffer_ptr);
        env.mem.free(buffer.audio_data);
        env.mem.free(buffer_ptr.cast());
    }

    if let Some(al_source) = host_object.al_source {
        unsafe {
            context.SourceStop(al_source);
            assert!(context.GetError() == 0);
        }

        unqueue_buffers(al_source, &context, |al_buffer| {
            host_object.al_unused_buffers.push(al_buffer)
        });

        unsafe {
            context.DeleteBuffers(
                host_object.al_unused_buffers.len().try_into().unwrap(),
                host_object.al_unused_buffers.as_ptr(),
            );
            assert!(context.GetError() == 0);
        }
    }

    ns_run_loop::remove_audio_queue(env, host_object.run_loop, in_aq);

    0 // success
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(AudioQueueNewOutput(_, _, _, _, _, _, _)),
    export_c_func!(AudioQueueGetParameter(_, _, _)),
    export_c_func!(AudioQueueSetParameter(_, _, _)),
    export_c_func!(AudioQueueAllocateBufferWithPacketDescriptions(_, _, _, _)),
    export_c_func!(AudioQueueAllocateBuffer(_, _, _)),
    export_c_func!(AudioQueueEnqueueBuffer(_, _, _, _)),
    export_c_func!(AudioQueueEnqueueBufferWithParameters(
        _,
        _,
        _,
        _,
        _,
        _,
        _,
        _,
        _,
        _
    )),
    export_c_func!(AudioQueueAddPropertyListener(_, _, _, _)),
    export_c_func!(AudioQueueRemovePropertyListener(_, _, _, _)),
    export_c_func!(AudioQueueGetPropertySize(_, _, _)),
    export_c_func!(AudioQueueGetProperty(_, _, _, _)),
    export_c_func!(AudioQueueSetProperty(_, _, _, _)),
    export_c_func!(AudioQueueSetOfflineRenderFormat(_, _, _)),
    export_c_func!(AudioQueueOfflineRender(_, _, _, _)),
    export_c_func!(AudioQueuePrime(_, _, _)),
    export_c_func!(AudioQueueStart(_, _)),
    export_c_func!(AudioQueuePause(_)),
    export_c_func!(AudioQueueStop(_, _)),
    export_c_func!(AudioQueueReset(_)),
    export_c_func!(AudioQueueFlush(_)),
    export_c_func!(AudioQueueFreeBuffer(_, _)),
    export_c_func!(AudioQueueDispose(_, _)),
];

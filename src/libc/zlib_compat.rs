/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Host-side zlib inflate compatibility.
//!
//! Keep the 32-bit iOS z_stream ABI in guest memory while using a native
//! host zlib stream internally. This preserves the exact inflate flush
//! semantics expected by older iOS libpng builds.

use std::collections::HashMap;
use std::mem::size_of;
use std::ptr;

use crate::dyld::{export_c_func, FunctionExports};
use crate::mem::{ConstPtr, MutPtr, Ptr};
use crate::Environment;

const Z_OK: i32 = 0;
const Z_STREAM_ERROR: i32 = -2;
const Z_VERSION_ERROR: i32 = -6;

const Z_STREAM_SIZE_IOS32: i32 = 56;

struct HostInflateStream {
    stream: Box<libz_sys::z_stream>,
    initialized: bool,
}

impl Drop for HostInflateStream {
    fn drop(&mut self) {
        if self.initialized {
            // SAFETY: stream was successfully initialized by zlib and remains
            // owned by this object until it is dropped.
            unsafe {
                libz_sys::inflateEnd(&mut *self.stream);
            }
        }
    }
}

#[derive(Default)]
pub struct State {
    streams: HashMap<u32, HostInflateStream>,
}


unsafe extern "C" fn host_zalloc(
    _opaque: libz_sys::voidpf,
    items: libz_sys::uInt,
    size: libz_sys::uInt,
) -> libz_sys::voidpf {
    unsafe { libc::calloc(items as usize, size as usize) }
}

unsafe extern "C" fn host_zfree(_opaque: libz_sys::voidpf, address: libz_sys::voidpf) {
    unsafe {
        libc::free(address);
    }
}

fn validate_init_args(
    env: &Environment,
    stream: MutPtr<u8>,
    version: ConstPtr<u8>,
    stream_size: i32,
) -> Result<(), i32> {
    if stream.is_null() {
        return Err(Z_STREAM_ERROR);
    }
    if version.is_null() || stream_size != Z_STREAM_SIZE_IOS32 || env.mem.read(version) != b'1' {
        return Err(Z_VERSION_ERROR);
    }
    Ok(())
}

fn write_metadata(
    env: &mut Environment,
    guest_stream: MutPtr<u8>,
    host_stream: &libz_sys::z_stream,
) {
    let null: MutPtr<u8> = Ptr::null();
    env.mem.write((guest_stream + 0x18).cast(), null);
    env.mem.write(
        (guest_stream + 0x08).cast(),
        u32::try_from(host_stream.total_in).unwrap(),
    );
    env.mem.write(
        (guest_stream + 0x14).cast(),
        u32::try_from(host_stream.total_out).unwrap(),
    );
    env.mem
        .write((guest_stream + 0x2c).cast(), host_stream.data_type);
    env.mem.write(
        (guest_stream + 0x30).cast(),
        u32::try_from(host_stream.adler).unwrap(),
    );
}

fn initialize_host_stream(
    env: &mut Environment,
    guest_stream: MutPtr<u8>,
    window_bits: i32,
) -> i32 {
    env.libc_state
        .zlib_compat
        .streams
        .remove(&guest_stream.to_bits());

    let mut host_stream = Box::new(libz_sys::z_stream {
        next_in: ptr::null_mut(),
        avail_in: 0,
        total_in: 0,
        next_out: ptr::null_mut(),
        avail_out: 0,
        total_out: 0,
        msg: ptr::null_mut(),
        state: ptr::null_mut(),
        zalloc: host_zalloc,
        zfree: host_zfree,
        opaque: ptr::null_mut(),
        data_type: 0,
        adler: 0,
        reserved: 0,
    });

    // SAFETY: host_stream points to valid writable storage. zlibVersion()
    // supplies the matching host zlib version string and stream size.
    let result = unsafe {
        libz_sys::inflateInit2_(
            &mut *host_stream,
            window_bits,
            libz_sys::zlibVersion(),
            i32::try_from(size_of::<libz_sys::z_stream>()).unwrap(),
        )
    };
    if result != Z_OK {
        return result;
    }

    write_metadata(env, guest_stream, &host_stream);

    // Keep guest z_stream::state non-null for code that checks it.
    env.mem.write((guest_stream + 0x1c).cast(), guest_stream);

    env.libc_state.zlib_compat.streams.insert(
        guest_stream.to_bits(),
        HostInflateStream {
            stream: host_stream,
            initialized: true,
        },
    );

    log_once!("Using native host zlib inflate compatibility");
    Z_OK
}

fn inflateInit_(
    env: &mut Environment,
    stream: MutPtr<u8>,
    version: ConstPtr<u8>,
    stream_size: i32,
) -> i32 {
    if let Err(error) = validate_init_args(env, stream, version, stream_size) {
        return error;
    }
    initialize_host_stream(env, stream, 15)
}

fn inflateInit2_(
    env: &mut Environment,
    stream: MutPtr<u8>,
    window_bits: i32,
    version: ConstPtr<u8>,
    stream_size: i32,
) -> i32 {
    if let Err(error) = validate_init_args(env, stream, version, stream_size) {
        return error;
    }
    initialize_host_stream(env, stream, window_bits)
}

fn inflate(env: &mut Environment, guest_stream: MutPtr<u8>, flush: i32) -> i32 {
    if guest_stream.is_null() {
        return Z_STREAM_ERROR;
    }

    let key = guest_stream.to_bits();
    let Some(mut host) = env.libc_state.zlib_compat.streams.remove(&key) else {
        return Z_STREAM_ERROR;
    };

    let next_in: MutPtr<u8> = env.mem.read(guest_stream.cast());
    let avail_in: u32 = env.mem.read((guest_stream + 0x04).cast());
    let next_out: MutPtr<u8> = env.mem.read((guest_stream + 0x0c).cast());
    let avail_out: u32 = env.mem.read((guest_stream + 0x10).cast());

    if (avail_in != 0 && next_in.is_null()) || (avail_out != 0 && next_out.is_null()) {
        env.libc_state.zlib_compat.streams.insert(key, host);
        return Z_STREAM_ERROR;
    }

    let mut input = if avail_in == 0 {
        Vec::new()
    } else {
        env.mem.bytes_at(next_in, avail_in).to_vec()
    };
    let mut output = vec![0u8; avail_out as usize];

    host.stream.next_in = if input.is_empty() {
        ptr::null_mut()
    } else {
        input.as_mut_ptr()
    };
    host.stream.avail_in = avail_in;
    host.stream.next_out = if output.is_empty() {
        ptr::null_mut()
    } else {
        output.as_mut_ptr()
    };
    host.stream.avail_out = avail_out;

    // SAFETY: input/output storage remains alive and fixed for the duration of
    // this call, and host.stream was initialized successfully by zlib.
    let result = unsafe { libz_sys::inflate(&mut *host.stream, flush) };

    let consumed = avail_in - host.stream.avail_in;
    let produced = avail_out - host.stream.avail_out;

    if produced != 0 {
        env.mem
            .bytes_at_mut(next_out, produced)
            .copy_from_slice(&output[..produced as usize]);
    }

    if !next_in.is_null() {
        env.mem.write(guest_stream.cast(), next_in + consumed);
    }
    env.mem
        .write((guest_stream + 0x04).cast(), host.stream.avail_in);

    if !next_out.is_null() {
        env.mem
            .write((guest_stream + 0x0c).cast(), next_out + produced);
    }
    env.mem
        .write((guest_stream + 0x10).cast(), host.stream.avail_out);

    write_metadata(env, guest_stream, &host.stream);

    if result < 0 {
        log!(
            "Native host zlib inflate returned {} after consuming {} and producing {} bytes",
            result,
            consumed,
            produced
        );
    }

    env.libc_state.zlib_compat.streams.insert(key, host);
    result
}

fn inflateReset(env: &mut Environment, guest_stream: MutPtr<u8>) -> i32 {
    if guest_stream.is_null() {
        return Z_STREAM_ERROR;
    }

    let key = guest_stream.to_bits();
    let Some(mut host) = env.libc_state.zlib_compat.streams.remove(&key) else {
        return Z_STREAM_ERROR;
    };

    // SAFETY: this host stream is initialized and owned by the map entry.
    let result = unsafe { libz_sys::inflateReset(&mut *host.stream) };
    if result == Z_OK {
        write_metadata(env, guest_stream, &host.stream);
    }

    env.libc_state.zlib_compat.streams.insert(key, host);
    result
}

fn inflateEnd(env: &mut Environment, guest_stream: MutPtr<u8>) -> i32 {
    if guest_stream.is_null() {
        return Z_STREAM_ERROR;
    }

    let Some(mut host) = env
        .libc_state
        .zlib_compat
        .streams
        .remove(&guest_stream.to_bits())
    else {
        return Z_STREAM_ERROR;
    };

    // SAFETY: this stream was initialized successfully and has not ended yet.
    let result = unsafe { libz_sys::inflateEnd(&mut *host.stream) };
    host.initialized = false;

    let null: MutPtr<u8> = Ptr::null();
    env.mem.write((guest_stream + 0x1c).cast(), null);
    result
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(inflateInit_(_, _, _)),
    export_c_func!(inflateInit2_(_, _, _, _)),
    export_c_func!(inflate(_, _)),
    export_c_func!(inflateReset(_)),
    export_c_func!(inflateEnd(_)),
];

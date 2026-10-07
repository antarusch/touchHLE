/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Host-side zlib inflate compatibility.
//!
//! Some older iOS applications exercise edge cases in the bundled guest zlib
//! that are difficult to bridge safely through a host callback. For ordinary
//! zlib and raw-deflate streams, keep the z_stream ABI in guest memory but do
//! the actual streaming decompression with flate2 on the host. Unsupported
//! inflateInit2 modes continue to fall back to the bundled guest libz.

use std::collections::HashMap;

use flate2::{Decompress, FlushDecompress, Status};

use crate::abi::{CallFromHost, GuestFunction};
use crate::dyld::{export_c_func, FunctionExports};
use crate::mem::{ConstPtr, MutPtr, Ptr};
use crate::Environment;

const Z_OK: i32 = 0;
const Z_STREAM_END: i32 = 1;
const Z_NEED_DICT: i32 = 2;
const Z_STREAM_ERROR: i32 = -2;
const Z_DATA_ERROR: i32 = -3;
const Z_BUF_ERROR: i32 = -5;
const Z_VERSION_ERROR: i32 = -6;

const Z_STREAM_SIZE_IOS32: i32 = 56;

struct HostInflateStream {
    decompress: Decompress,
    zlib_header: bool,
}

#[derive(Default)]
pub struct State {
    streams: HashMap<u32, HostInflateStream>,
}

fn guest_zlib_function(env: &Environment, symbol: &str) -> GuestFunction {
    let bin = env
        .bins
        .iter()
        .find(|bin| bin.name.starts_with("libz.") || bin.name == "libz.dylib")
        .expect("zlib compatibility shim called without a loaded guest libz");
    let &addr = bin
        .exported_symbols
        .get(symbol)
        .unwrap_or_else(|| panic!("Guest zlib does not export {symbol}"));
    GuestFunction::from_addr_with_thumb_bit(addr)
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

fn initialize_host_stream(env: &mut Environment, stream: MutPtr<u8>, zlib_header: bool) -> i32 {
    let key = stream.to_bits();
    env.libc_state.zlib_compat.streams.insert(
        key,
        HostInflateStream {
            decompress: Decompress::new(zlib_header),
            zlib_header,
        },
    );

    let null: MutPtr<u8> = Ptr::null();
    env.mem.write((stream + 0x08).cast(), 0u32); // total_in
    env.mem.write((stream + 0x14).cast(), 0u32); // total_out
    env.mem.write((stream + 0x18).cast(), null); // msg

    // Keep z_stream::state non-null; the real state lives in the host map.
    env.mem.write((stream + 0x1c).cast(), stream);

    env.mem.write((stream + 0x2c).cast(), 0i32); // data_type
    env.mem.write((stream + 0x30).cast(), 1u32); // adler

    log_once!("Using host-side zlib inflate compatibility");
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
    initialize_host_stream(env, stream, true)
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

    // zlib uses positive 8..=15 for a zlib header and negative values for raw
    // deflate. Those are the modes Crimson and the large majority of older iOS
    // games use. Preserve the guest dylib for gzip/auto-detect modes.
    if (8..=15).contains(&window_bits) || window_bits == 0 {
        return initialize_host_stream(env, stream, true);
    }
    if (-15..=-8).contains(&window_bits) {
        return initialize_host_stream(env, stream, false);
    }

    env.libc_state.zlib_compat.streams.remove(&stream.to_bits());
    let real_init = guest_zlib_function(env, "_inflateInit2_");
    real_init.call_from_host(env, (stream, window_bits, version, stream_size))
}

fn inflate(env: &mut Environment, stream: MutPtr<u8>, flush: i32) -> i32 {
    if stream.is_null() {
        return Z_STREAM_ERROR;
    }

    let key = stream.to_bits();
    let Some(mut host_stream) = env.libc_state.zlib_compat.streams.remove(&key) else {
        let real_inflate = guest_zlib_function(env, "_inflate");
        return real_inflate.call_from_host(env, (stream, flush));
    };

    let next_in: MutPtr<u8> = env.mem.read(stream.cast());
    let avail_in: u32 = env.mem.read((stream + 0x04).cast());
    let next_out: MutPtr<u8> = env.mem.read((stream + 0x0c).cast());
    let avail_out: u32 = env.mem.read((stream + 0x10).cast());

    if (avail_in != 0 && next_in.is_null()) || (avail_out != 0 && next_out.is_null()) {
        env.libc_state.zlib_compat.streams.insert(key, host_stream);
        return Z_STREAM_ERROR;
    }

    // Copy guest buffers so the backend can run without holding a mutable
    // borrow of guest memory. This also lets one guest inflate() call invoke
    // the backend repeatedly to match zlib's "make as much progress as
    // possible" behavior.
    let input = if avail_in == 0 {
        Vec::new()
    } else {
        env.mem.bytes_at(next_in, avail_in).to_vec()
    };
    let mut output = vec![0u8; avail_out as usize];

    let before_in = host_stream.decompress.total_in();
    let before_out = host_stream.decompress.total_out();
    let flush_mode = match flush {
        4 => FlushDecompress::Finish,
        // flate2 has no Z_PARTIAL_FLUSH. Sync is the closest decompression
        // behavior for the older libpng used by some iOS games.
        1..=3 => FlushDecompress::Sync,
        _ => FlushDecompress::None,
    };

    let mut input_offset = 0usize;
    let mut output_offset = 0usize;
    let mut stream_ended = false;
    let mut error_code = None;

    loop {
        let call_before_in = host_stream.decompress.total_in();
        let call_before_out = host_stream.decompress.total_out();

        let result = host_stream.decompress.decompress(
            &input[input_offset..],
            &mut output[output_offset..],
            flush_mode,
        );

        let consumed_now =
            usize::try_from(host_stream.decompress.total_in() - call_before_in).unwrap();
        let produced_now =
            usize::try_from(host_stream.decompress.total_out() - call_before_out).unwrap();
        input_offset += consumed_now;
        output_offset += produced_now;

        match result {
            Ok(Status::StreamEnd) => {
                stream_ended = true;
                break;
            }
            Ok(Status::Ok | Status::BufError) => {}
            Err(error) => {
                let needs_dictionary = error.needs_dictionary().is_some();
                log!("Host zlib inflate error: {error}");
                error_code = Some(if needs_dictionary {
                    Z_NEED_DICT
                } else {
                    Z_DATA_ERROR
                });
                break;
            }
        }

        if output_offset == output.len() {
            break;
        }

        // zlib's inflate() keeps going internally while it can make progress.
        // In particular, a backend may have pending output even after it has
        // consumed all currently supplied input.
        if consumed_now == 0 && produced_now == 0 {
            break;
        }
    }

    let consumed = u32::try_from(host_stream.decompress.total_in() - before_in).unwrap();
    let produced = u32::try_from(host_stream.decompress.total_out() - before_out).unwrap();

    if produced != 0 {
        env.mem
            .bytes_at_mut(next_out, produced)
            .copy_from_slice(&output[..produced as usize]);
    }

    if !next_in.is_null() {
        env.mem.write(stream.cast(), next_in + consumed);
    }
    env.mem.write((stream + 0x04).cast(), avail_in - consumed);
    env.mem.write(
        (stream + 0x08).cast(),
        u32::try_from(host_stream.decompress.total_in()).unwrap(),
    );

    if !next_out.is_null() {
        env.mem.write((stream + 0x0c).cast(), next_out + produced);
    }
    env.mem.write((stream + 0x10).cast(), avail_out - produced);
    env.mem.write(
        (stream + 0x14).cast(),
        u32::try_from(host_stream.decompress.total_out()).unwrap(),
    );

    let return_value = if let Some(error) = error_code {
        error
    } else if stream_ended {
        Z_STREAM_END
    } else if (consumed == 0 && produced == 0) || flush == 4 {
        // zlib 1.2.x returns Z_BUF_ERROR when no progress was possible, and
        // also for Z_FINISH until the stream has actually ended.
        Z_BUF_ERROR
    } else {
        Z_OK
    };

    env.libc_state.zlib_compat.streams.insert(key, host_stream);
    return_value
}

fn inflateReset(env: &mut Environment, stream: MutPtr<u8>) -> i32 {
    if stream.is_null() {
        return Z_STREAM_ERROR;
    }

    if let Some(host_stream) = env
        .libc_state
        .zlib_compat
        .streams
        .get_mut(&stream.to_bits())
    {
        host_stream.decompress.reset(host_stream.zlib_header);
        env.mem.write((stream + 0x08).cast(), 0u32);
        env.mem.write((stream + 0x14).cast(), 0u32);
        env.mem.write((stream + 0x30).cast(), 1u32);
        return Z_OK;
    }

    let real_reset = guest_zlib_function(env, "_inflateReset");
    real_reset.call_from_host(env, (stream,))
}

fn inflateEnd(env: &mut Environment, stream: MutPtr<u8>) -> i32 {
    if stream.is_null() {
        return Z_STREAM_ERROR;
    }

    if env
        .libc_state
        .zlib_compat
        .streams
        .remove(&stream.to_bits())
        .is_some()
    {
        let null: MutPtr<u8> = Ptr::null();
        env.mem.write((stream + 0x1c).cast(), null);
        return Z_OK;
    }

    let real_end = guest_zlib_function(env, "_inflateEnd");
    real_end.call_from_host(env, (stream,))
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(inflateInit_(_, _, _)),
    export_c_func!(inflateInit2_(_, _, _, _)),
    export_c_func!(inflate(_, _)),
    export_c_func!(inflateReset(_)),
    export_c_func!(inflateEnd(_)),
];

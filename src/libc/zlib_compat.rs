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
                                                 // A non-null sentinel keeps code that checks z_stream::state happy. The
                                                 // actual inflate state lives in the host-side map above.
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

    // Copy input so the guest memory can be borrowed mutably for output at the
    // same time without aliasing Rust references.
    let input = if avail_in == 0 {
        Vec::new()
    } else {
        env.mem.bytes_at(next_in, avail_in).to_vec()
    };

    let before_in = host_stream.decompress.total_in();
    let before_out = host_stream.decompress.total_out();
    let flush_mode = if flush == 4 {
        FlushDecompress::Finish
    } else {
        // zlib's inflate() only gives Z_FINISH special return-code semantics.
        FlushDecompress::None
    };

    let result = if avail_out == 0 {
        let mut empty = [];
        host_stream
            .decompress
            .decompress(&input, &mut empty, flush_mode)
    } else {
        let output = env.mem.bytes_at_mut(next_out, avail_out);
        host_stream
            .decompress
            .decompress(&input, output, flush_mode)
    };

    let consumed = u32::try_from(host_stream.decompress.total_in() - before_in).unwrap();
    let produced = u32::try_from(host_stream.decompress.total_out() - before_out).unwrap();

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

    let return_value = match result {
        Ok(Status::Ok) => Z_OK,
        Ok(Status::StreamEnd) => Z_STREAM_END,
        Ok(Status::BufError) => Z_BUF_ERROR,
        Err(error) => {
            let needs_dictionary = error.needs_dictionary().is_some();
            log!("Host zlib inflate error: {error}");
            if needs_dictionary {
                Z_NEED_DICT
            } else {
                Z_DATA_ERROR
            }
        }
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

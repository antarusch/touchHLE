/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Compatibility shims for touchHLE's bundled guest zlib.
//!
//! The bundled iOS zlib remains the implementation. These wrappers only
//! correct guest/runtime ABI state when an otherwise-valid stream would be
//! rejected because the inflate state advertises a smaller window than the
//! zlib header requires.

use crate::abi::{CallFromHost, GuestFunction};
use crate::dyld::{export_c_func, FunctionExports};
use crate::mem::{MutPtr, Ptr};
use crate::Environment;

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

fn inflate(env: &mut Environment, stream: MutPtr<u8>, flush: i32) -> i32 {
    // z_stream (32-bit iOS ABI):
    //   +0x00 next_in, +0x04 avail_in, ... +0x1c state.
    // zlib 1.2.3 inflate_state stores wbits at +0x24.
    if !stream.is_null() {
        let next_in: MutPtr<u8> = env.mem.read(stream.cast());
        let avail_in: u32 = env.mem.read((stream + 4).cast());
        let state: MutPtr<u8> = env.mem.read((stream + 0x1c).cast());

        if !next_in.is_null() && avail_in >= 2 && !state.is_null() {
            let cmf = env.mem.read(next_in);
            let flg = env.mem.read(next_in + 1);
            let method = cmf & 0x0f;
            let required_wbits = u32::from(cmf >> 4) + 8;
            let header = (u16::from(cmf) << 8) | u16::from(flg);
            let valid_zlib_header =
                method == 8 && header % 31 == 0 && (8..=15).contains(&required_wbits);

            if valid_zlib_header {
                let wbits_ptr: MutPtr<u32> = (state + 0x24).cast();
                let current_wbits = env.mem.read(wbits_ptr);
                log_once!(
                    "zlib inflate header requires {} window bits; guest state has {}",
                    required_wbits,
                    current_wbits
                );

                if current_wbits < required_wbits {
                    log!(
                        "Correcting bundled zlib inflate window from {} to {} bits",
                        current_wbits,
                        required_wbits
                    );
                    env.mem.write(wbits_ptr, required_wbits);
                }
            } else {
                log_once!(
                    "zlib inflate first bytes are {:#04x} {:#04x}; not applying window compatibility correction",
                    cmf,
                    flg
                );
            }
        }
    }

    let real_inflate = guest_zlib_function(env, "_inflate");
    real_inflate.call_from_host(env, (stream, flush))
}

pub const FUNCTIONS: FunctionExports = &[export_c_func!(inflate(_, _))];

/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Apple System Log (ASL) compatibility.
//!
//! Older iOS applications and bundled SDKs often use ASL for diagnostic
//! logging. Android/touchHLE has no ASL daemon, so clients are represented by
//! lightweight guest handles and messages are forwarded to touchHLE's log.

use crate::abi::DotDotDot;
use crate::dyld::{export_c_func, FunctionExports};
use crate::libc::stdio::printf::printf_inner;
use crate::mem::{ConstPtr, MutPtr, MutVoidPtr, Ptr};
use crate::Environment;

type AslClient = MutPtr<u8>;
type AslMsg = MutVoidPtr;

fn asl_open(
    env: &mut Environment,
    ident: ConstPtr<u8>,
    facility: ConstPtr<u8>,
    opts: u32,
) -> AslClient {
    let ident = if ident.is_null() {
        None
    } else {
        Some(env.mem.cstr_at_utf8(ident).to_string())
    };
    let facility = if facility.is_null() {
        None
    } else {
        Some(env.mem.cstr_at_utf8(facility).to_string())
    };

    log_dbg!(
        "asl_open(ident={:?}, facility={:?}, opts={:#x})",
        ident,
        facility,
        opts
    );

    // ASL clients are opaque to applications. A one-byte guest allocation
    // provides a stable non-NULL handle until asl_close().
    env.mem.alloc(1).cast()
}

fn asl_close(env: &mut Environment, client: AslClient) {
    if !client.is_null() {
        env.mem.free(client.cast());
    }
}

fn asl_log(
    env: &mut Environment,
    _client: AslClient,
    _msg: AslMsg,
    level: i32,
    format: ConstPtr<u8>,
    args: DotDotDot,
) -> i32 {
    if format.is_null() {
        return 0;
    }

    let rendered =
        printf_inner::<false, _>(env, |mem, idx| mem.read(format + idx), args.start());
    log!(
        "Guest ASL [{}]: {}",
        level,
        String::from_utf8_lossy(&rendered).trim_end_matches(['\r', '\n'])
    );
    0
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(asl_open(_, _, _)),
    export_c_func!(asl_close(_)),
    export_c_func!(asl_log(_, _, _, _, _)),
];

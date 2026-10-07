/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Minimal Grand Central Dispatch compatibility.
//!
//! touchHLE is currently single-threaded from the guest application's point of
//! view. Queue submissions therefore execute inline while preserving the GCD
//! API shape expected by older iOS applications.

use crate::abi::{CallFromHost, GuestFunction};
use crate::dyld::{export_c_func, ConstantExports, FunctionExports, HostConstant};
use crate::mem::{ConstVoidPtr, MutPtr};
use crate::Environment;

#[derive(Default)]
pub struct State {
    main_queue: Option<MutPtr<u8>>,
    global_queue: Option<MutPtr<u8>>,
}

fn allocate_queue(env: &mut Environment) -> MutPtr<u8> {
    env.mem.calloc(16).cast()
}

fn main_queue(env: &mut Environment) -> MutPtr<u8> {
    if let Some(queue) = env.libc_state.dispatch.main_queue {
        queue
    } else {
        let queue = allocate_queue(env);
        env.libc_state.dispatch.main_queue = Some(queue);
        queue
    }
}

fn global_queue(env: &mut Environment) -> MutPtr<u8> {
    if let Some(queue) = env.libc_state.dispatch.global_queue {
        queue
    } else {
        let queue = allocate_queue(env);
        env.libc_state.dispatch.global_queue = Some(queue);
        queue
    }
}

fn main_queue_symbol(env: &mut Environment) -> ConstVoidPtr {
    main_queue(env).cast_const().cast()
}

pub const CONSTANTS: ConstantExports =
    &[("__dispatch_main_q", HostConstant::Custom(main_queue_symbol))];

fn dispatch_get_global_queue(env: &mut Environment, _identifier: i32, _flags: u32) -> MutPtr<u8> {
    log_once!("Using synchronous Grand Central Dispatch compatibility");
    global_queue(env)
}

fn invoke_block(env: &mut Environment, block: MutPtr<u8>) {
    if block.is_null() {
        return;
    }

    // 32-bit Apple Blocks ABI:
    // isa, flags, reserved, invoke, descriptor.
    let invoke_addr: u32 = env.mem.read((block + 12).cast());
    if invoke_addr == 0 {
        log!(
            "Warning: dispatch block {:?} has a null invoke function",
            block
        );
        return;
    }

    let invoke = GuestFunction::from_addr_with_thumb_bit(invoke_addr);
    let _: () = invoke.call_from_host(env, (block,));
}

fn dispatch_async(env: &mut Environment, _queue: MutPtr<u8>, block: MutPtr<u8>) {
    invoke_block(env, block);
}

fn dispatch_sync(env: &mut Environment, _queue: MutPtr<u8>, block: MutPtr<u8>) {
    invoke_block(env, block);
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(dispatch_get_global_queue(_, _)),
    export_c_func!(dispatch_async(_, _)),
    export_c_func!(dispatch_sync(_, _)),
];

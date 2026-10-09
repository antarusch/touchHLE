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
use crate::environment::ThreadBlock;
use crate::mem::{ConstVoidPtr, MutPtr, MutVoidPtr};
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

// 32-bit Apple Blocks ABI:
// isa, flags, reserved, invoke, descriptor.
fn block_invoke_function(env: &mut Environment, block: MutPtr<u8>) -> Option<GuestFunction> {
    if block.is_null() {
        return None;
    }
    let invoke_addr: u32 = env.mem.read((block + 12).cast());
    if invoke_addr == 0 {
        log!("Warning: block {:?} has a null invoke function", block);
        return None;
    }
    Some(GuestFunction::from_addr_with_thumb_bit(invoke_addr))
}

pub(crate) fn invoke_block(env: &mut Environment, block: MutPtr<u8>) {
    if let Some(invoke) = block_invoke_function(env, block) {
        let _: () = invoke.call_from_host(env, (block,));
    }
}

pub(crate) fn invoke_block_with_bool(env: &mut Environment, block: MutPtr<u8>, value: bool) {
    if let Some(invoke) = block_invoke_function(env, block) {
        let _: () = invoke.call_from_host(env, (block, value));
    }
}

/// A completed 32-bit libdispatch predicate is all-one bits.
pub const DISPATCH_ONCE_DONE: u32 = u32::MAX;
const DISPATCH_ONCE_RUNNING: u32 = 1;

#[derive(Debug, PartialEq, Eq)]
enum OnceAction {
    Run,
    Wait,
    Done,
}

fn once_action(value: u32) -> OnceAction {
    match value {
        0 => OnceAction::Run,
        DISPATCH_ONCE_DONE => OnceAction::Done,
        _ => OnceAction::Wait,
    }
}

/// Run the initializer synchronously, blocking other emulated threads until
/// the same predicate is complete. This also supports dispatch_once_f.
fn dispatch_once_common(
    env: &mut Environment,
    predicate: MutPtr<u32>,
    initializer: impl FnOnce(&mut Environment),
) {
    assert!(
        !predicate.is_null(),
        "dispatch_once called with a null predicate"
    );
    match once_action(env.mem.read(predicate)) {
        OnceAction::Done => {}
        OnceAction::Wait => env.yield_thread(ThreadBlock::DispatchOnce(predicate)),
        OnceAction::Run => {
            env.mem.write(predicate, DISPATCH_ONCE_RUNNING);
            initializer(env);
            env.mem.write(predicate, DISPATCH_ONCE_DONE);
        }
    }
}

fn dispatch_once(env: &mut Environment, predicate: MutPtr<u32>, block: MutPtr<u8>) {
    assert!(!block.is_null(), "dispatch_once called with a null block");
    dispatch_once_common(env, predicate, |env| invoke_block(env, block));
}

fn dispatch_once_f(
    env: &mut Environment,
    predicate: MutPtr<u32>,
    context: MutVoidPtr,
    work: GuestFunction,
) {
    dispatch_once_common(env, predicate, |env| {
        let _: () = work.call_from_host(env, (context,));
    });
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
    export_c_func!(dispatch_once(_, _)),
    export_c_func!(dispatch_once_f(_, _, _)),
];

#[cfg(test)]
mod tests {
    use super::{once_action, OnceAction, DISPATCH_ONCE_DONE, DISPATCH_ONCE_RUNNING};

    #[test]
    fn once_predicate_states() {
        assert_eq!(once_action(0), OnceAction::Run);
        assert_eq!(once_action(DISPATCH_ONCE_RUNNING), OnceAction::Wait);
        assert_eq!(once_action(DISPATCH_ONCE_DONE), OnceAction::Done);
    }
}

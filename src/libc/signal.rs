/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use crate::dyld::{export_c_func, FunctionExports};
use crate::libc::errno::{set_errno, EINVAL, ENOMEM};
use crate::mem::{ConstPtr, ConstVoidPtr, GuestUSize, MutPtr, MutVoidPtr, Ptr, SafeRead};
use crate::{Environment, ThreadId};
use std::collections::HashMap;

/// Darwin's 32-bit stack_t layout (sp, size, flags), not the Linux layout.
#[allow(non_camel_case_types)]
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
struct stack_t {
    ss_sp: MutVoidPtr,
    ss_size: GuestUSize,
    ss_flags: i32,
}
unsafe impl SafeRead for stack_t {}

const SS_DISABLE: i32 = 0x0004;
const MINSIGSTKSZ: GuestUSize = 32 * 1024;

impl stack_t {
    fn disabled() -> Self {
        Self {
            ss_sp: Ptr::null(),
            ss_size: 0,
            ss_flags: SS_DISABLE,
        }
    }
}

/// The alternate signal stack is per-thread. Signal delivery on this stack
/// is not yet implemented, but applications can register and query it.
#[derive(Default)]
pub struct State {
    stacks: HashMap<ThreadId, stack_t>,
}

fn validate_alt_stack(new: stack_t) -> Result<(), i32> {
    if new.ss_flags == SS_DISABLE {
        // ss_sp and ss_size are ignored when disabling the alternate stack.
        return Ok(());
    }
    if new.ss_flags != 0 || new.ss_sp.is_null() {
        return Err(EINVAL);
    }
    if new.ss_size < MINSIGSTKSZ {
        return Err(ENOMEM);
    }
    Ok(())
}

/// Query, register, or disable the calling thread's alternate signal stack.
/// SS_ONSTACK is never reported until emulated signal delivery is implemented.
fn sigaltstack(env: &mut Environment, ss: ConstPtr<stack_t>, old_ss: MutPtr<stack_t>) -> i32 {
    // Read the new value before writing old_ss, because guest pointers can alias.
    let requested = if ss.is_null() {
        None
    } else {
        Some(env.mem.read(ss))
    };

    if let Some(new_stack) = requested {
        if let Err(err) = validate_alt_stack(new_stack) {
            set_errno(env, err);
            return -1;
        }
    }

    let thread = env.current_thread;
    let current = env
        .libc_state
        .signal
        .stacks
        .get(&thread)
        .copied()
        .unwrap_or_else(stack_t::disabled);
    if !old_ss.is_null() {
        env.mem.write(old_ss, current);
    }
    if let Some(new_stack) = requested {
        if new_stack.ss_flags == SS_DISABLE {
            env.libc_state.signal.stacks.remove(&thread);
        } else {
            env.libc_state.signal.stacks.insert(thread, new_stack);
        }
    }
    0
}


fn sigaction(env: &mut Environment, signum: i32, act: ConstVoidPtr, old_act: MutVoidPtr) -> i32 {
    // TODO: handle errno properly
    set_errno(env, 0);

    log!("TODO: sigaction({:?}, {:?}, {:?})", signum, act, old_act);
    0
}

fn signal(env: &mut Environment, signum: i32, handler: MutVoidPtr) -> MutVoidPtr {
    // TODO: handle errno properly
    set_errno(env, 0);

    log!("TODO: signal({:?}, {:?})", signum, handler);
    Ptr::null()
}

fn sigprocmask(env: &mut Environment, how: i32, set: ConstVoidPtr, old_set: MutVoidPtr) -> i32 {
    // TODO: handle errno properly
    set_errno(env, 0);

    log!("TODO: sigprocmask({}, {:?}, {:?})", how, set, old_set);
    0
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(sigaltstack(_, _)),
    export_c_func!(sigaction(_, _, _)),
    export_c_func!(signal(_, _)),
    export_c_func!(sigprocmask(_, _, _)),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn darwin_arm32_stack_layout() {
        assert_eq!(std::mem::size_of::<stack_t>(), 12);
    }

    #[test]
    fn alternate_stack_arguments() {
        let valid = stack_t {
            ss_sp: Ptr::from_bits(0x1000),
            ss_size: MINSIGSTKSZ,
            ss_flags: 0,
        };
        assert_eq!(validate_alt_stack(valid), Ok(()));
        assert_eq!(
            validate_alt_stack(stack_t {
                ss_size: MINSIGSTKSZ - 1,
                ..valid
            }),
            Err(ENOMEM)
        );
        assert_eq!(
            validate_alt_stack(stack_t {
                ss_flags: 0x0001,
                ..valid
            }),
            Err(EINVAL)
        );
        assert_eq!(
            validate_alt_stack(stack_t {
                ss_sp: Ptr::null(),
                ..valid
            }),
            Err(EINVAL)
        );
        assert_eq!(validate_alt_stack(stack_t::disabled()), Ok(()));
    }
}

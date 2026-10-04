/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSInvocation`.

use crate::abi::{extend_stack_for_args, write_next_arg};
use crate::cpu::Cpu;
use crate::frameworks::foundation::{NSInteger, NSUInteger};
use crate::libc::string::strdup;
use crate::mem::{ConstPtr, MutPtr, MutVoidPtr};
use crate::msg;
use crate::objc::{
    autorelease, id, nil, objc_classes, objc_msgSend, release, retain, ClassExports, HostObject,
    SEL,
};

struct NSInvocationHostObject {
    /// `NSMethodSignature *`
    sig: id,
    return_value: Vec<u8>,
    /// Argument type strings resolved from `sig` at creation time
    argument_types: Vec<String>,
    target: id,
    selector: Option<SEL>,
    /// Per-slot owned buffer for each argument.
    /// Option denotes if argument was set with `setArgument:atIndex:`
    arguments: Vec<Option<MutVoidPtr>>,
    arguments_retained: bool,
    /// Objects retained by `retainArguments`
    retained_objects: Vec<id>,
    /// C string copies made by `retainArguments`
    copied_strings: Vec<MutPtr<u8>>,
}
impl HostObject for NSInvocationHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSInvocation: NSObject

+ (id)invocationWithMethodSignature:(id)sig {
    // NSMethodSignature *
    retain(env, sig);
    let num_of_args: NSUInteger = msg![env; sig numberOfArguments];
    let mut argument_types: Vec<String> = Vec::with_capacity(num_of_args as usize);
    for i in 0..num_of_args {
        let type_ptr: ConstPtr<u8> = msg![env; sig getArgumentTypeAtIndex:i];
        argument_types.push(env.mem.cstr_at_utf8(type_ptr).unwrap().to_string());
    }
    let host_object = Box::new(NSInvocationHostObject {
        sig,
        return_value: Vec::new(),
        argument_types,
        target: nil,
        selector: None,
        arguments: vec![None; num_of_args as usize],
        arguments_retained: false,
        retained_objects: Vec::new(),
        copied_strings: Vec::new(),
    });
    let res = env.objc.alloc_object(this, host_object, &mut env.mem);
    autorelease(env, res)
}

- (id)target {
    env.objc.borrow::<NSInvocationHostObject>(this).target
}
- (())setTarget:(id)target {
    let old_target = env.objc.borrow::<NSInvocationHostObject>(this).target;
    let arguments_retained = env.objc.borrow::<NSInvocationHostObject>(this).arguments_retained;
    env.objc.borrow_mut::<NSInvocationHostObject>(this).target = target;
    if arguments_retained {
        retain(env, target);
        release(env, old_target);
    }
}

- (())setSelector:(SEL)selector {
    env.objc.borrow_mut::<NSInvocationHostObject>(this).selector = Some(selector);
}

- (())retainArguments {
    // TODO: handle return val
    // TODO: copy blocks
    if env
        .objc
        .borrow::<NSInvocationHostObject>(this)
        .arguments_retained
    {
        return;
    }

    let target = env.objc.borrow::<NSInvocationHostObject>(this).target;
    retain(env, target);

    let mut retained_objects: Vec<id> = Vec::new();
    let mut copied_strings: Vec<MutPtr<u8>> = Vec::new();

    // Skip index 0 (self) and 1 (SEL): handled via target/selector fields.
    let num_of_args = env
        .objc
        .borrow::<NSInvocationHostObject>(this)
        .argument_types
        .len();
    for i in 2..num_of_args {
        let host = env.objc.borrow::<NSInvocationHostObject>(this);
        let Some(arg_loc) = host.arguments[i] else {
            continue;
        };
        match host.argument_types[i].as_str() {
            "@" => {
                let obj: id = env.mem.read(arg_loc.cast().cast_const());
                retain(env, obj);
                retained_objects.push(obj);
            }
            "*" => {
                let str: MutPtr<u8> = env.mem.read(arg_loc.cast().cast_const());
                let str_copy = strdup(env, str.cast_const());
                env.mem.write(arg_loc.cast(), str_copy);
                copied_strings.push(str_copy);
            }
            _ => {}
        }
    }

    let host = env.objc.borrow_mut::<NSInvocationHostObject>(this);
    host.retained_objects = retained_objects;
    host.copied_strings = copied_strings;
    host.arguments_retained = true;
}

- (())setArgument:(MutVoidPtr)arg_loc
          atIndex:(NSInteger)idx {
    let NSInvocationHostObject { arguments, .. } = env.objc.borrow::<NSInvocationHostObject>(this);

    // 0 and 1 are reserved for `self` and `_cmd`
    // TODO: can they be set too?
    assert!(1 < idx && idx < arguments.len() as NSInteger);

    if let Some(prev_arg) = arguments[idx as usize] {
        env.mem.free(prev_arg.cast());
    }

    let argument_types: &Vec<String> = env
        .objc
        .borrow::<NSInvocationHostObject>(this)
        .argument_types
        .as_ref();
    let arg_type = argument_types.get(idx as usize).unwrap();
    let size = super::type_encoding::parse(arg_type.as_bytes()).1;
    let bytes = env.mem.bytes_at(arg_loc.cast(), size).to_vec();
    let new = env.mem.alloc(size);
    env.mem
        .bytes_at_mut(new.cast(), size)
        .copy_from_slice(&bytes);

    env.objc
        .borrow_mut::<NSInvocationHostObject>(this)
        .arguments[idx as usize] = Some(new);
}

- (())invokeWithTarget:(id)target {
    () = msg![env; this setTarget:target];
    () = msg![env; this invoke];
}

- (())invoke {
    // Safeguard: all arguments must be set (except first two)
    let arguments: &Vec<Option<MutVoidPtr>> = env
        .objc
        .borrow::<NSInvocationHostObject>(this)
        .arguments
        .as_ref();
    let set_count = arguments.iter().flatten().count();
    let all_count = arguments.len();
    assert_eq!(set_count + 2, all_count);

    let host = env.objc.borrow::<NSInvocationHostObject>(this);
    let sig = host.sig;
    let types = host.argument_types.clone();
    let args = host.arguments.clone();
    let target = host.target;
    let selector = host.selector.unwrap();
    let return_type: ConstPtr<u8> = msg![env; sig methodReturnType];
    let return_type = env.mem.cstr_at_utf8(return_type).unwrap().to_string();
    let return_size = super::type_encoding::parse(return_type.as_bytes()).1;
    let indirect = return_size > 4 && return_type.starts_with(['{', '[', '(']);
    let return_buffer = if indirect {
        env.mem.alloc(return_size)
    } else {
        MutVoidPtr::null()
    };
    let mut count = u32::from(indirect) as usize;
    for typ in &types {
        let (_, size, _) = super::type_encoding::parse(typ.as_bytes());
        count += size.div_ceil(4) as usize;
    }
    let old_sp = extend_stack_for_args(count, env.cpu.regs_mut());
    let mut offset = 0;
    if indirect {
        write_next_arg(&mut offset, env.cpu.regs_mut(), &mut env.mem, return_buffer);
    }
    for (index, typ) in types.iter().enumerate() {
        if index == 0 {
            write_next_arg(&mut offset, env.cpu.regs_mut(), &mut env.mem, target);
            continue;
        }
        if index == 1 {
            write_next_arg(&mut offset, env.cpu.regs_mut(), &mut env.mem, selector);
            continue;
        }
        let (_, size, _) = super::type_encoding::parse(typ.as_bytes());
        let bytes = env.mem.bytes_at(args[index].unwrap().cast(), size).to_vec();
        for chunk in bytes.chunks(4) {
            let mut word = [0u8; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            write_next_arg(
                &mut offset,
                env.cpu.regs_mut(),
                &mut env.mem,
                u32::from_le_bytes(word),
            );
        }
    }
    if indirect {
        crate::objc::objc_msgSend_stret(env, return_buffer, target, selector);
    } else {
        objc_msgSend(env, target, selector);
    }
    let return_value = if indirect {
        let value = env.mem.bytes_at(return_buffer.cast(), return_size).to_vec();
        env.mem.free(return_buffer);
        value
    } else {
        let mut value = Vec::new();
        for word in &env.cpu.regs()[..return_size.div_ceil(4) as usize] {
            value.extend_from_slice(&word.to_le_bytes());
        }
        value.truncate(return_size as usize);
        value
    };
    env.cpu.regs_mut()[Cpu::SP] = old_sp;
    env.objc
        .borrow_mut::<NSInvocationHostObject>(this)
        .return_value = return_value;
}

- (())getReturnValue:(MutVoidPtr)buffer {
    let value = env
        .objc
        .borrow::<NSInvocationHostObject>(this)
        .return_value
        .clone();
    env.mem
        .bytes_at_mut(buffer.cast(), value.len() as u32)
        .copy_from_slice(&value);
}

- (())dealloc {
    let &NSInvocationHostObject { sig, target, arguments_retained, .. } = env.objc.borrow::<NSInvocationHostObject>(this);
    release(env, sig);
    if arguments_retained {
        release(env, target);
        let retained_objects = std::mem::take(
            &mut env.objc.borrow_mut::<NSInvocationHostObject>(this).retained_objects
        );
        for obj in retained_objects {
            release(env, obj);
        }
        let copied_strings = std::mem::take(
            &mut env.objc.borrow_mut::<NSInvocationHostObject>(this).copied_strings
        );
        for s in copied_strings {
            env.mem.free(s.cast());
        }
    } else {
        assert!(env.objc.borrow::<NSInvocationHostObject>(this).retained_objects.is_empty());
        assert!(env.objc.borrow::<NSInvocationHostObject>(this).copied_strings.is_empty());
    }
    for ptr in env.objc.borrow::<NSInvocationHostObject>(this).arguments.iter().flatten() {
        env.mem.free(ptr.cast());
    }
    env.objc.dealloc_object(this, &mut env.mem)
}

@end

};

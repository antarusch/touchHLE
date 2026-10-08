/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `CFUUID`.

use super::cf_allocator::{kCFAllocatorDefault, CFAllocatorRef};
use super::cf_string::CFStringRef;
use super::CFTypeRef;
use crate::dyld::{export_c_func, FunctionExports};
use crate::frameworks::foundation::ns_string::from_rust_string;
use crate::objc::{objc_classes, ClassExports, HostObject};
use crate::Environment;
use uuid::Uuid;

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

// CFUUID doesn't have a corresponding NS type (at least, not up until iOS 6+
// and even that one is _not_ toll-free bridged, see NSUUID docs),
// but the callers of CFUUIDCreate() are expected to call CFRelease() on them.
@implementation _touchHLE_CFUUID: NSObject
@end

};

/// Note: Apple is using a pointer to an opaque struct instead
type CFUUIDRef = CFTypeRef;

struct CFUUIDHostObject {
    uuid: Uuid,
}
impl HostObject for CFUUIDHostObject {}

fn create_uuid(env: &mut Environment, uuid: Uuid) -> CFUUIDRef {
    let host_obj = Box::new(CFUUIDHostObject { uuid });
    let class = env.objc.get_known_class("_touchHLE_CFUUID", &mut env.mem);
    env.objc.alloc_object(class, host_obj, &mut env.mem)
}

fn CFUUIDCreate(env: &mut Environment, allocator: CFAllocatorRef) -> CFUUIDRef {
    assert!(allocator == kCFAllocatorDefault || env.mem.read(allocator).is_system_default()); // unimplemented

    create_uuid(env, Uuid::new_v4())
}

fn CFUUIDCreateFromUUIDBytes(
    env: &mut Environment,
    allocator: CFAllocatorRef,
    word0: u32,
    word1: u32,
    word2: u32,
    word3: u32,
) -> CFUUIDRef {
    assert!(allocator == kCFAllocatorDefault || env.mem.read(allocator).is_system_default()); // unimplemented

    // CFUUIDBytes is a 16-byte struct passed by value. On 32-bit ARM, after
    // the allocator in r0, its first three words arrive in r1-r3 and its last
    // word arrives on the stack. Modeling the struct as four u32 arguments
    // gives the same ABI layout in touchHLE's guest-call bridge.
    let mut bytes = [0u8; 16];
    bytes[0..4].copy_from_slice(&word0.to_le_bytes());
    bytes[4..8].copy_from_slice(&word1.to_le_bytes());
    bytes[8..12].copy_from_slice(&word2.to_le_bytes());
    bytes[12..16].copy_from_slice(&word3.to_le_bytes());

    create_uuid(env, Uuid::from_bytes(bytes))
}

fn CFUUIDCreateString(
    env: &mut Environment,
    allocator: CFAllocatorRef,
    uuid: CFUUIDRef,
) -> CFStringRef {
    assert!(allocator == kCFAllocatorDefault || env.mem.read(allocator).is_system_default()); // unimplemented

    let host_object = env.objc.borrow::<CFUUIDHostObject>(uuid);
    let uuid_str = host_object.uuid.hyphenated().to_string().to_uppercase();
    from_rust_string(env, uuid_str)
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(CFUUIDCreate(_)),
    export_c_func!(CFUUIDCreateFromUUIDBytes(_, _, _, _, _)),
    export_c_func!(CFUUIDCreateString(_, _)),
];

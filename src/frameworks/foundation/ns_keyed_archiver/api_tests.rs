/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Host API integration checks, sharing one headless emulator environment.
use crate::frameworks::core_animation::ca_transform_3d::CATransform3D;
use crate::frameworks::core_graphics::{CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::ns_string::get_static_str;
use crate::mem::MutVoidPtr;
use crate::objc::{autorelease, id, msg, msg_class, nil, objc_classes, release, ClassExports};
use crate::options::Options;
use crate::Environment;

pub const CLASSES: ClassExports = objc_classes! {
(env, this, _cmd);
@implementation ArchiveProbe: NSObject
- (())encodeWithCoder:(id)coder {
    let flag = get_static_str(env, "flag");
    let value = get_static_str(env, "value");
    let back = get_static_str(env, "back");
    () = msg![env; coder encodeBool:true forKey:flag];
    () = msg![env; coder encodeFloat:0.625f32 forKey:value];
    () = msg![env; coder encodeObject:this forKey:back];
}
- (id)initWithCoder:(id)coder {
    let flag = get_static_str(env, "flag");
    let value = get_static_str(env, "value");
    let back = get_static_str(env, "back");
    let decoded_flag: bool = msg![env; coder decodeBoolForKey:flag];
    let decoded_value: f32 = msg![env; coder decodeFloatForKey:value];
    let decoded_back: id = msg![env; coder decodeObjectForKey:back];
    assert!(decoded_flag);
    assert_eq!(decoded_value, 0.625);
    assert_eq!(decoded_back, this);
    this
}
- (f64)add:(i32)integer double:(f64)double {
    f64::from(integer) + double
}
- (CGRect)rectangle {
    CGRect {
        origin: CGPoint { x: 2.0, y: 3.0 },
        size: CGSize {
            width: 4.0,
            height: 5.0,
        },
    }
}
@end
@implementation TableProbe: NSObject
- (i32)tableView:(id)_table numberOfRowsInSection:(i32)_section {
    3
}
- (id)tableView:(id)table cellForRowAtIndexPath:(id)_path {
    let identifier = get_static_str(env, "row");
    let reused: id = msg![env; table dequeueReusableCellWithIdentifier:identifier];
    if reused != nil {
        return reused;
    }
    let cell: id = msg_class![env; UITableViewCell alloc];
    let cell: id = msg![env; cell initWithStyle:0i32 reuseIdentifier:identifier];
    autorelease(env, cell)
}
@end
};

fn invocation(env: &mut Environment, target: id, types: &[u8], name: &str) -> id {
    let encoding = env.mem.alloc_and_write_cstr(types).cast_const();
    let signature: id = msg_class![env; NSMethodSignature signatureWithObjCTypes:encoding];
    env.mem.free(encoding.cast_mut().cast());
    let invocation: id = msg_class![env; NSInvocation invocationWithMethodSignature:signature];
    let selector = env.objc.register_host_selector(name.into(), &mut env.mem);
    () = msg![env; invocation setTarget:target];
    () = msg![env; invocation setSelector:selector];
    invocation
}

#[test]
fn archive_invocation_and_menu_round_trips() {
    let options = Options {
        headless: true,
        direct_memory_access: false,
        ..Default::default()
    };
    let icon = crate::image::Image::from_pixel_vec(vec![0; 4], (1, 1));
    let mut env = Environment::new_without_app(options, icon).unwrap();
    let env = &mut env;
    let pool: id = msg_class![env; NSAutoreleasePool new];
    crate::frameworks::foundation::ns_dictionary::check_copy_unset_layer_actions(env);
    // Eustrath reads a custom transaction key before any layer mutation.
    let transaction_key = get_static_str(env, "EustrathTransactionProbe");
    let unset: id = msg_class![env; CATransaction valueForKey:transaction_key];
    assert_eq!(unset, nil);
    let duration: f64 = msg_class![env; CATransaction animationDuration];
    assert_eq!(duration, 0.25);
    let disabled: bool = msg_class![env; CATransaction disableActions];
    assert!(!disabled);
    () = msg_class![env; CATransaction flush];
    // The setter must also initialize a transaction after flushing.
    () = msg_class![env; CATransaction setAnimationDuration:0.75f64];
    () = msg_class![env; CATransaction setDisableActions:true];
    let transaction_value: id = msg_class![env; NSNumber numberWithInt:42i32];
    () = msg_class![env; CATransaction setValue:transaction_value forKey:transaction_key];
    let actual: id = msg_class![env; CATransaction valueForKey:transaction_key];
    assert_eq!(actual, transaction_value);
    let duration: f64 = msg_class![env; CATransaction animationDuration];
    assert_eq!(duration, 0.75);
    let disabled: bool = msg_class![env; CATransaction disableActions];
    assert!(disabled);
    () = msg_class![env; CATransaction flush];
    let data: id = msg_class![env; NSMutableData new];
    let encoder: id = msg_class![env; NSKeyedArchiver alloc];
    let encoder: id = msg![env; encoder initForWritingWithMutableData:data];
    let root = get_static_str(env, "root");
    let absent = get_static_str(env, "absent");
    let shared = get_static_str(env, "shared");
    let probe: id = msg_class![env; ArchiveProbe new];
    let float: id = msg_class![env; NSNumber numberWithFloat:0.125f32];
    let unsigned: id = msg_class![env; NSNumber numberWithUnsignedLongLong:(u64::MAX)];
    crate::objc::retain(env, float);
    crate::objc::retain(env, float);
    crate::objc::retain(env, unsigned);
    let array =
        crate::frameworks::foundation::ns_array::from_vec(env, vec![float, float, unsigned]);
    () = msg![env; encoder encodeObject:probe forKey:root];
    () = msg![env; encoder encodeObject:nil forKey:absent];
    () = msg![env; encoder encodeObject:array forKey:shared];
    () = msg![env; encoder finishEncoding];
    () = msg![env; encoder finishEncoding];
    let length: u32 = msg![env; data length];
    let bytes: crate::mem::ConstVoidPtr = msg![env; data bytes];
    assert_eq!(env.mem.bytes_at(bytes.cast(), 8), b"bplist00");
    assert!(length > 8);
    let decoder: id = msg_class![env; NSKeyedUnarchiver alloc];
    let decoder: id = msg![env; decoder initForReadingWithData:data];
    let decoded: id = msg![env; decoder decodeObjectForKey:root];
    assert_ne!(decoded, probe);
    let null: id = msg![env; decoder decodeObjectForKey:absent];
    assert_eq!(null, nil);
    let decoded_array: id = msg![env; decoder decodeObjectForKey:shared];
    let first: id = msg![env; decoded_array objectAtIndex:0u32];
    let second: id = msg![env; decoded_array objectAtIndex:1u32];
    let third: id = msg![env; decoded_array objectAtIndex:2u32];
    assert_eq!(first, second);
    let decoded_float: f64 = msg![env; first doubleValue];
    let decoded_unsigned: u64 = msg![env; third unsignedLongLongValue];
    assert_eq!(decoded_float, 0.125);
    assert_eq!(decoded_unsigned, u64::MAX);
    () = msg![env; decoder finishDecoding];
    release(env, decoder);
    let decoded_root: id = msg_class![env; NSKeyedUnarchiver unarchiveObjectWithData:data];
    let class = env.objc.get_known_class("ArchiveProbe", &mut env.mem);
    assert!(msg![env; decoded_root isKindOfClass:class]);

    // A double after an odd number of argument words must cross r3/stack.
    let invoke = invocation(env, probe, b"d20@0:4i8d12\0", "add:double:");
    let integer = env.mem.alloc_and_write(7i32);
    let double = env.mem.alloc_and_write(0.25f64);
    () = msg![env; invoke setArgument:(integer.cast::<std::ffi::c_void>()) atIndex:2i32];
    () = msg![env; invoke setArgument:(double.cast::<std::ffi::c_void>()) atIndex:3i32];
    () = msg![env; invoke invoke];
    let result: MutVoidPtr = env.mem.alloc(64);
    () = msg![env; invoke getReturnValue:result];
    assert_eq!(env.mem.read::<f64, false>(result.cast().cast_const()), 7.25);
    let invoke = invocation(
        env,
        probe,
        b"{CGRect={CGPoint=ff}{CGSize=ff}}8@0:4\0",
        "rectangle",
    );
    () = msg![env; invoke invoke];
    () = msg![env; invoke getReturnValue:result];
    let rect: CGRect = env.mem.read(result.cast().cast_const());
    assert_eq!({ rect.origin.x }, 2.0);
    assert_eq!({ rect.size.height }, 5.0);
    let transform = CATransform3D::translation(6.0, 7.0, 8.0);
    let value: id = msg_class![env; NSValue valueWithCATransform3D:transform];
    () = msg![env; value getValue:result];
    assert_eq!(
        env.mem
            .read::<CATransform3D, false>(result.cast().cast_const()),
        transform
    );

    let table: id = msg_class![env; UITableView alloc];
    let table: id = msg![env; table initWithFrame:(CGRect { origin: CGPoint::default(), size: CGSize { width: 200.0, height: 100.0 } }) style:0i32];
    let source: id = msg_class![env; TableProbe new];
    () = msg![env; table setDataSource:source];
    () = msg![env; table reloadData];
    let size: CGSize = msg![env; table contentSize];
    assert_eq!({ size.height }, 132.0);
    let path: id = msg_class![env; NSIndexPath indexPathForRow:1i32 inSection:0i32];
    let cell: id = msg![env; table cellForRowAtIndexPath:path];
    () = msg![env; table selectRowAtIndexPath:path animated:false scrollPosition:0i32];
    let selected: bool = msg![env; cell isSelected];
    assert!(selected);
    let selected_path: id = msg![env; table indexPathForSelectedRow];
    assert_eq!(selected_path, path);
    () = msg![env; table reloadData];
    let reused: id = msg![env; table cellForRowAtIndexPath:path];
    assert_eq!(reused, cell);
    let selected: bool = msg![env; reused isSelected];
    assert!(!selected);
    let slider: id = msg_class![env; UISlider new];
    () = msg![env; slider setMinimumValue:0.25f32];
    () = msg![env; slider setValue:2.0f32];
    let value: f32 = msg![env; slider value];
    assert_eq!(value, 1.0);
    () = msg![env; slider setValue:0.0f32];
    let value: f32 = msg![env; slider value];
    assert_eq!(value, 0.25);

    let type_encoding = env
        .mem
        .alloc_and_write_cstr(b"{CATransform3D=ffffffffffffffff}");
    let value: id = msg_class![env; NSValue valueWithBytes:(result.cast_const()) objCType:(type_encoding.cast_const())];
    let decoded_transform: CATransform3D = msg![env; value CATransform3DValue];
    assert_eq!(decoded_transform, transform);
    env.mem.free(type_encoding.cast());

    crate::frameworks::core_animation::integration_check(env);
    crate::frameworks::core_graphics::cg_context::integration_check(env);
    for object in [encoder, data, probe, array, table, source, slider] {
        release(env, object);
    }
    for pointer in [integer.cast(), double.cast(), result] {
        env.mem.free(pointer);
    }
    release(env, pool);
}

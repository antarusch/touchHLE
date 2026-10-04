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
    check_property_lists(env);
    check_nib_scroll_view(env);
    check_set_archives(env);
    check_foundation_archives(env);
    check_text_nib(env);
    // A token reader can inspect the full source without moving its cursor.
    let scanner_input = get_static_str(env, " Name42");
    let scanner: id = msg_class![env; NSScanner alloc];
    let scanner: id = msg![env; scanner initWithString:scanner_input];
    let scanner_text: id = msg![env; scanner string];
    assert!(msg![env; scanner_text isEqualToString:scanner_input]);
    let location: u32 = msg![env; scanner scanLocation];
    assert_eq!(location, 0);
    () = msg![env; scanner setScanLocation:5u32];
    let full_text: id = msg![env; scanner string];
    assert_eq!(full_text, scanner_text);
    let location: u32 = msg![env; scanner scanLocation];
    assert_eq!(location, 5);
    crate::objc::retain(env, scanner_text);
    release(env, scanner);
    assert!(msg![env; scanner_text isEqualToString:scanner_input]);
    release(env, scanner_text);
    let label: id = msg_class![env; UILabel alloc];
    let frame = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize {
            width: 20.0,
            height: 20.0,
        },
    };
    let label: id = msg![env; label initWithFrame:frame];
    assert!(msg![env; label isEnabled]);
    let text_color: id = msg![env; label textColor];
    () = msg![env; label setEnabled:false];
    assert!(!msg![env; label isEnabled]);
    let disabled_color: id = msg![env; label textColor];
    assert_eq!(disabled_color, text_color);
    () = msg![env; label setEnabled:true];
    assert!(msg![env; label isEnabled]);
    release(env, label);
    // The IPA uses both collection methods when stages and effects finish.
    let member: id = msg_class![env; NSObject new];
    let array: id = msg_class![env; NSArray arrayWithObject:member];
    let set: id = msg_class![env; NSMutableSet new];
    () = msg![env; set addObjectsFromArray:array];
    () = msg![env; set addObjectsFromArray:array];
    () = msg![env; set addObjectsFromArray:nil];
    let count: u32 = msg![env; set count];
    assert_eq!(count, 1);
    assert!(msg![env; set containsObject:member]);
    let before: u32 = msg![env; member retainCount];
    let inner_pool: id = msg_class![env; NSAutoreleasePool new];
    let selector = env
        .objc
        .register_host_selector("retain".into(), &mut env.mem);
    () = msg![env; set makeObjectsPerformSelector:selector];
    release(env, inner_pool);
    let after: u32 = msg![env; member retainCount];
    assert_eq!(after, before + 1);
    release(env, member); // The retain callback's reference.
    release(env, set);
    release(env, member);
    // An allObjects snapshot keeps members alive after the set is destroyed.
    for class_name in ["NSSet", "NSMutableSet"] {
        let inner_pool: id = msg_class![env; NSAutoreleasePool new];
        let member: id = msg_class![env; NSObject new];
        let class = env.objc.get_known_class(class_name, &mut env.mem);
        let set: id = msg![env; class alloc];
        let set: id = msg![env; set initWithObject:member];
        let snapshot: id = msg![env; set allObjects];
        release(env, set);
        release(env, member);
        let saved: id = msg![env; snapshot objectAtIndex:0u32];
        assert_eq!(saved, member);
        let references: u32 = msg![env; saved retainCount];
        assert_eq!(references, 1);
        release(env, inner_pool);
    }
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

fn check_nib_scroll_view(env: &mut Environment) {
    use nibarchive::{ClassName, NIBArchive, Object, Value, ValueVariant};
    // The geometry payloads match Eustrath's ChatPanel UIScrollView.
    let geometry = |components: &[f32]| {
        let mut bytes = vec![6];
        for component in components {
            bytes.extend(component.to_le_bytes());
        }
        ValueVariant::Data(bytes)
    };
    let archive = NIBArchive::new(
        vec![Object::new(0, 0, 4)],
        vec![
            "UIBounds".into(),
            "UICenter".into(),
            "UIContentSize".into(),
            "negative".into(),
        ],
        vec![
            Value::new(0, geometry(&[0.0, 0.0, 580.0, 124.0])),
            Value::new(1, geometry(&[357.0, 117.0])),
            Value::new(2, geometry(&[580.0, 37.0])),
            Value::new(3, geometry(&[-1.0, -1.5])),
        ],
        vec![ClassName::new("UIScrollView".into(), vec![])],
    )
    .unwrap()
    .to_bytes();
    let bytes = env.mem.alloc(archive.len().try_into().unwrap());
    env.mem
        .bytes_at_mut(bytes.cast(), archive.len().try_into().unwrap())
        .copy_from_slice(&archive);
    let data: id =
        msg_class![env; NSData dataWithBytes:(bytes.cast_const()) length:(archive.len() as u32)];
    env.mem.free(bytes);
    let coder: id = msg_class![env; _touchHLE_NIBArchiveDecoder alloc];
    let coder: id = msg![env; coder _touchHLE_initForReadingWithData:data];
    let key = get_static_str(env, "UIContentSize");
    let size: CGSize = msg![env; coder decodeCGSizeForKey:key];
    assert_eq!((size.width, size.height), (580.0, 37.0));
    let key = get_static_str(env, "negative");
    let size: CGSize = msg![env; coder decodeCGSizeForKey:key];
    assert_eq!((size.width, size.height), (-1.0, -1.5));
    let key = get_static_str(env, "absent");
    let size: CGSize = msg![env; coder decodeCGSizeForKey:key];
    assert_eq!((size.width, size.height), (0.0, 0.0));
    let scroll: id = msg_class![env; UIScrollView alloc];
    let scroll: id = msg![env; scroll initWithCoder:coder];
    let size: CGSize = msg![env; scroll contentSize];
    assert_eq!((size.width, size.height), (580.0, 37.0));
    let bounds: CGRect = msg![env; scroll bounds];
    assert_eq!((bounds.size.width, bounds.size.height), (580.0, 124.0));
    let center: CGPoint = msg![env; scroll center];
    assert_eq!((center.x, center.y), (357.0, 117.0));
    release(env, scroll);
    release(env, coder);
}

fn check_set_graph(env: &mut Environment, decoded: id, class_name: &str) {
    let restored: id = msg![env; decoded objectAtIndex:0u32];
    let member: id = msg![env; decoded objectAtIndex:1u32];
    let repeated: id = msg![env; decoded objectAtIndex:2u32];
    let class = env.objc.get_known_class(class_name, &mut env.mem);
    assert!(msg![env; restored isKindOfClass:class]);
    assert_eq!(restored, repeated);
    let count: u32 = msg![env; restored count];
    assert_eq!(count, 2);
    assert!(msg![env; restored containsObject:member]);
    let members: id = msg![env; restored allObjects];
    let first: id = msg![env; members objectAtIndex:0u32];
    let second: id = msg![env; members objectAtIndex:1u32];
    assert!(first == member || second == member);
    if class_name == "NSCountedSet" {
        let occurrences: u32 = msg![env; restored countForObject:member];
        assert_eq!(occurrences, 2);
        let other = if first == member { second } else { first };
        let occurrences: u32 = msg![env; restored countForObject:other];
        assert_eq!(occurrences, 1);
    }
    if class_name == "NSMutableSet" {
        let extra = get_static_str(env, "gamma");
        () = msg![env; restored addObject:extra];
        let count: u32 = msg![env; restored count];
        assert_eq!(count, 3);
    }
}

fn data_from_bytes(env: &mut Environment, contents: &[u8]) -> id {
    let length = contents.len().try_into().unwrap();
    let bytes = env.mem.alloc(length);
    env.mem
        .bytes_at_mut(bytes.cast(), length)
        .copy_from_slice(contents);
    let data: id = msg_class![env; NSData dataWithBytes:(bytes.cast_const()) length:length];
    env.mem.free(bytes);
    data
}

fn check_set_archives(env: &mut Environment) {
    let alpha = get_static_str(env, "alpha");
    let beta = get_static_str(env, "beta");
    let objects = vec![
        crate::objc::retain(env, alpha),
        crate::objc::retain(env, beta),
        crate::objc::retain(env, alpha),
    ];
    let members = crate::frameworks::foundation::ns_array::from_vec(env, objects);
    for class_name in ["NSSet", "NSMutableSet", "NSCountedSet"] {
        let class = env.objc.get_known_class(class_name, &mut env.mem);
        let set: id = msg![env; class alloc];
        let set: id = msg![env; set initWithArray:members];
        let first = crate::objc::retain(env, set);
        let shared = crate::objc::retain(env, alpha);
        let repeated = crate::objc::retain(env, set);
        let root =
            crate::frameworks::foundation::ns_array::from_vec(env, vec![first, shared, repeated]);
        let data: id = msg_class![env; NSKeyedArchiver archivedDataWithRootObject:root];
        let decoded: id = msg_class![env; NSKeyedUnarchiver unarchiveObjectWithData:data];
        check_set_graph(env, decoded, class_name);
        let count: u32 = msg![env; set count];
        assert_eq!(count, 2);
        let empty: id = msg![env; class new];
        let data: id = msg_class![env; NSKeyedArchiver archivedDataWithRootObject:empty];
        let restored: id = msg_class![env; NSKeyedUnarchiver unarchiveObjectWithData:data];
        let count: u32 = msg![env; restored count];
        assert_eq!(count, 0);
        assert!(msg![env; restored isKindOfClass:class]);
        for object in [set, root, empty] {
            release(env, object);
        }
    }
    release(env, members);
    // CI's native Foundation test produces an independent Apple archive.
    if let Some(path) = std::env::var_os("TOUCHHLE_SET_FIXTURE") {
        let contents = std::fs::read(path).unwrap();
        let data = data_from_bytes(env, &contents);
        let decoded: id = msg_class![env; NSKeyedUnarchiver unarchiveObjectWithData:data];
        for (index, class_name) in ["NSSet", "NSMutableSet", "NSCountedSet"].iter().enumerate() {
            let root: id = msg![env; decoded objectAtIndex:(index as u32)];
            check_set_graph(env, root, class_name);
        }
    }
}

fn check_text_nib(env: &mut Environment) {
    use nibarchive::{ClassName, NIBArchive, Object, Value, ValueVariant};
    let geometry = |components: &[f32]| {
        let mut bytes = vec![6];
        for component in components {
            bytes.extend(component.to_le_bytes());
        }
        ValueVariant::Data(bytes)
    };
    let archive = NIBArchive::new(
        vec![
            Object::new(0, 0, 8),
            Object::new(1, 8, 3),
            Object::new(2, 11, 2),
            Object::new(3, 13, 1),
            Object::new(3, 14, 1),
        ],
        vec![
            "UIBounds",
            "UICenter",
            "UIContentSize",
            "UITextColor",
            "UIFont",
            "UIText",
            "UITextAlignment",
            "UIEditable",
            "UIColorComponentCount",
            "UIWhite",
            "UIAlpha",
            "UIFontName",
            "UIFontPointSize",
            "NS.bytes",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        vec![
            Value::new(0, geometry(&[0.0, 0.0, 400.0, 100.0])),
            Value::new(1, geometry(&[200.0, 50.0])),
            Value::new(2, geometry(&[400.0, 100.0])),
            Value::new(3, ValueVariant::ObjectRef(1)),
            Value::new(4, ValueVariant::ObjectRef(2)),
            Value::new(5, ValueVariant::ObjectRef(3)),
            Value::new(6, ValueVariant::Int8(1)),
            Value::new(7, ValueVariant::Bool(false)),
            Value::new(8, ValueVariant::Int8(2)),
            Value::new(9, ValueVariant::Float(1.0)),
            Value::new(10, ValueVariant::Float(1.0)),
            Value::new(11, ValueVariant::ObjectRef(4)),
            Value::new(12, ValueVariant::Double(24.0)),
            Value::new(13, ValueVariant::Data(b"Bright dialogue".to_vec())),
            Value::new(13, ValueVariant::Data(b"ArialMT".to_vec())),
        ],
        ["UITextView", "UIColor", "UIFont", "NSString"]
            .into_iter()
            .map(|name| ClassName::new(name.into(), vec![]))
            .collect(),
    )
    .unwrap()
    .to_bytes();
    let data = data_from_bytes(env, &archive);
    let coder: id = msg_class![env; _touchHLE_NIBArchiveDecoder alloc];
    let coder: id = msg![env; coder _touchHLE_initForReadingWithData:data];
    let view: id = msg_class![env; UITextView alloc];
    let view: id = msg![env; view initWithCoder:coder];
    let color: id = msg![env; view textColor];
    assert_eq!(
        crate::frameworks::uikit::ui_color::get_rgba(&env.objc, color),
        (1.0, 1.0, 1.0, 1.0)
    );
    let font: id = msg![env; view font];
    let point_size: f32 = msg![env; font pointSize];
    assert_eq!(point_size, 24.0);
    let text: id = msg![env; view text];
    assert_eq!(
        crate::frameworks::foundation::ns_string::to_rust_string(env, text),
        "Bright dialogue"
    );
    let alignment: i32 = msg![env; view textAlignment];
    assert_eq!(alignment, 1);
    assert!(!msg![env; view isEditable]);
    // Verify real text drawing produces white pixels, not black glyphs.
    use crate::frameworks::core_graphics::{
        cg_bitmap_context, cg_color_space, cg_context, cg_image,
    };
    use crate::frameworks::uikit::ui_graphics;
    let pixels = env.mem.calloc(400 * 100 * 4);
    let space = cg_color_space::CGColorSpaceCreateDeviceRGB(env);
    let context = cg_bitmap_context::CGBitmapContextCreate(
        env,
        pixels,
        400,
        100,
        8,
        400 * 4,
        space,
        cg_image::kCGImageAlphaPremultipliedLast,
    );
    ui_graphics::UIGraphicsPushContext(env, context);
    let bounds: CGRect = msg![env; view bounds];
    () = msg![env; view drawRect:bounds];
    ui_graphics::UIGraphicsPopContext(env);
    let bytes = env.mem.bytes_at(pixels.cast(), 400 * 100 * 4);
    assert!(bytes
        .chunks_exact(4)
        .any(|pixel| pixel[0] > 0 && pixel[1] > 0 && pixel[2] > 0));
    cg_context::CGContextRelease(env, context);
    cg_color_space::CGColorSpaceRelease(env, space);
    env.mem.free(pixels);
    // StageTitlePanel expands a narrow label before centring its full title.
    let label: id = msg_class![env; UILabel alloc];
    let initial = CGRect {
        origin: CGPoint { x: 10.0, y: 20.0 },
        size: CGSize {
            width: 30.0,
            height: 20.0,
        },
    };
    let label: id = msg![env; label initWithFrame:initial];
    let title = get_static_str(env, "An Unexpected Encounter");
    () = msg![env; label setFont:font];
    () = msg![env; label setText:title];
    let expected: CGSize = msg![env; title sizeWithFont:font];
    () = msg![env; label sizeToFit];
    let fitted: CGRect = msg![env; label frame];
    assert_eq!((fitted.origin.x, fitted.origin.y), (10.0, 20.0));
    assert_eq!(
        (fitted.size.width, fitted.size.height),
        (expected.width, expected.height)
    );
    assert!(fitted.size.width > initial.size.width);
    for object in [label, view, coder] {
        release(env, object);
    }
}

fn check_foundation_graph(env: &mut Environment, root: id) {
    let text = get_static_str(env, "Café 火 😀");
    let mutable: id = msg![env; root objectAtIndex:0u32];
    let immutable: id = msg![env; root objectAtIndex:1u32];
    let repeated: id = msg![env; root objectAtIndex:2u32];
    let class = env.objc.get_known_class("NSMutableString", &mut env.mem);
    assert!(msg![env; mutable isKindOfClass:class]);
    assert!(msg![env; mutable isEqualToString:text]);
    assert!(msg![env; immutable isEqualToString:text]);
    assert_eq!(mutable, repeated);
    let suffix = get_static_str(env, "!");
    () = msg![env; mutable appendString:suffix];
    let changed = get_static_str(env, "Café 火 😀!");
    assert!(msg![env; mutable isEqualToString:changed]);
    assert!(msg![env; immutable isEqualToString:text]);
    for index in [3u32, 4] {
        let data: id = msg![env; root objectAtIndex:index];
        let bytes = crate::frameworks::foundation::ns_data::to_rust_slice(env, data);
        assert_eq!(bytes, &[0, 1, 0, 255]);
        let class_name = if index == 3 {
            "NSData"
        } else {
            "NSMutableData"
        };
        let class = env.objc.get_known_class(class_name, &mut env.mem);
        assert!(msg![env; data isKindOfClass:class]);
    }
    let data: id = msg![env; root objectAtIndex:4u32];
    let byte = env.mem.alloc_and_write(7u8);
    () = msg![env; data appendBytes:(byte.cast_const()) length:1u32];
    env.mem.free(byte.cast());
    assert_eq!(
        crate::frameworks::foundation::ns_data::to_rust_slice(env, data),
        &[0, 1, 0, 255, 7]
    );
    let date: id = msg![env; root objectAtIndex:5u32];
    let time: f64 = msg![env; date timeIntervalSinceReferenceDate];
    assert_eq!(time, 1234.25);
    let null: id = msg_class![env; NSNull null];
    let decoded_null: id = msg![env; root objectAtIndex:6u32];
    assert_eq!(decoded_null, null);
    for index in [7u32, 8] {
        let data: id = msg![env; root objectAtIndex:index];
        let length: u32 = msg![env; data length];
        assert_eq!(length, 0);
        let class = env.objc.get_known_class(
            if index == 7 {
                "NSData"
            } else {
                "NSMutableData"
            },
            &mut env.mem,
        );
        assert!(msg![env; data isKindOfClass:class]);
        let mutable_class = env.objc.get_known_class("NSMutableData", &mut env.mem);
        let mutable: bool = msg![env; data isKindOfClass:mutable_class];
        assert_eq!(mutable, index == 8);
    }
}

fn check_foundation_archives(env: &mut Environment) {
    check_legacy_string_readers(env);
    let text = get_static_str(env, "Café 火 😀");
    let mutable: id = msg_class![env; NSMutableString alloc];
    let mutable: id = msg![env; mutable initWithString:text];
    let data = data_from_bytes(env, &[0, 1, 0, 255]);
    let mutable_data: id = msg_class![env; NSMutableData dataWithData:data];
    let empty = data_from_bytes(env, &[]);
    let empty_mutable: id = msg_class![env; NSMutableData data];
    let date: id = msg_class![env; NSDate alloc];
    let date: id = msg![env; date initWithTimeIntervalSinceReferenceDate:1234.25f64];
    let null: id = msg_class![env; NSNull null];
    let objects = [
        mutable,
        text,
        mutable,
        data,
        mutable_data,
        date,
        null,
        empty,
        empty_mutable,
    ]
    .into_iter()
    .map(|object| crate::objc::retain(env, object))
    .collect();
    let root = crate::frameworks::foundation::ns_array::from_vec(env, objects);
    let encoded: id = msg_class![env; NSKeyedArchiver archivedDataWithRootObject:root];
    let decoded: id = msg_class![env; NSKeyedUnarchiver unarchiveObjectWithData:encoded];
    check_foundation_graph(env, decoded);
    assert!(msg![env; mutable isEqualToString:text]);
    assert_eq!(
        crate::frameworks::foundation::ns_data::to_rust_slice(env, mutable_data),
        &[0, 1, 0, 255]
    );
    for object in [root, mutable, date] {
        release(env, object);
    }
    if let Some(path) = std::env::var_os("TOUCHHLE_FOUNDATION_FIXTURE") {
        let contents = std::fs::read(path).unwrap();
        let data = data_from_bytes(env, &contents);
        let root: id = msg_class![env; NSKeyedUnarchiver unarchiveObjectWithData:data];
        check_foundation_graph(env, root);
    }
}

fn check_legacy_string_readers(env: &mut Environment) {
    let text = get_static_str(env, "Café 火 😀");
    // Older keyed string objects use NS.bytes rather than NS.string.
    use plist::{Dictionary, Uid, Value};
    let mut object = Dictionary::new();
    object.insert(
        "NS.bytes".into(),
        Value::Data("Café 火 😀".as_bytes().to_vec()),
    );
    object.insert("$class".into(), Value::Uid(Uid::new(2)));
    let mut class = Dictionary::new();
    class.insert("$classname".into(), "NSMutableString".into());
    class.insert(
        "$classes".into(),
        Value::Array(
            ["NSMutableString", "NSString", "NSObject"]
                .into_iter()
                .map(|name| Value::String(name.into()))
                .collect(),
        ),
    );
    let mut top = Dictionary::new();
    top.insert("root".into(), Value::Uid(Uid::new(1)));
    let mut archive = Dictionary::new();
    archive.insert("$archiver".into(), "NSKeyedArchiver".into());
    archive.insert("$version".into(), 100000.into());
    archive.insert(
        "$objects".into(),
        Value::Array(vec!["$null".into(), object.into(), class.into()]),
    );
    archive.insert("$top".into(), top.into());
    let mut bytes = Vec::new();
    plist::to_writer_binary(&mut bytes, &archive).unwrap();
    let data = data_from_bytes(env, &bytes);
    let legacy: id = msg_class![env; NSKeyedUnarchiver unarchiveObjectWithData:data];
    let class = env.objc.get_known_class("NSMutableString", &mut env.mem);
    assert!(msg![env; legacy isKindOfClass:class]);
    assert!(msg![env; legacy isEqualToString:text]);
    // NIB strings use the byte form too; init must keep the mutable receiver.
    use nibarchive::{ClassName, NIBArchive, Object, Value as NibValue, ValueVariant};
    let archive = NIBArchive::new(
        vec![Object::new(0, 0, 1)],
        vec!["NS.bytes".into()],
        vec![NibValue::new(
            0,
            ValueVariant::Data("Café 火 😀".as_bytes().to_vec()),
        )],
        vec![ClassName::new("NSMutableString".into(), vec![])],
    )
    .unwrap()
    .to_bytes();
    let data = data_from_bytes(env, &archive);
    let coder: id = msg_class![env; _touchHLE_NIBArchiveDecoder alloc];
    let coder: id = msg![env; coder _touchHLE_initForReadingWithData:data];
    let allocated: id = msg_class![env; NSMutableString alloc];
    let decoded: id = msg![env; allocated initWithCoder:coder];
    assert_eq!(decoded, allocated);
    assert!(msg![env; decoded isEqualToString:text]);
    let suffix = get_static_str(env, "!");
    () = msg![env; decoded appendString:suffix];
    let changed = get_static_str(env, "Café 火 😀!");
    assert!(msg![env; decoded isEqualToString:changed]);
    release(env, decoded);
    release(env, coder);
}

fn check_property_lists(env: &mut Environment) {
    use crate::frameworks::foundation::ns_property_list_serialization::*;
    use crate::mem::MutPtr;
    use plist::{Dictionary, Value};
    let error_ptr: MutPtr<id> = env.mem.alloc(std::mem::size_of::<id>() as u32).cast();
    let format_ptr: MutPtr<u32> = env.mem.alloc(4).cast();
    env.mem.write(error_ptr, nil);
    // Reproduces the game's XML-format request, including NSString ** output.
    let root: id = msg_class![env; NSMutableDictionary dictionary];
    let key = get_static_str(env, "About Will");
    let seen: id = msg_class![env; NSNumber numberWithBool:true];
    () = msg![env; root setObject:seen forKey:key];
    let xml: id = msg_class![env; NSPropertyListSerialization dataFromPropertyList:root
        format:NSPropertyListXMLFormat_v1_0 errorDescription:error_ptr];
    assert_ne!(xml, nil);
    let bytes = crate::frameworks::foundation::ns_data::to_rust_slice(env, xml);
    let value = Value::from_reader_xml(std::io::Cursor::new(bytes)).unwrap();
    assert_eq!(
        value.as_dictionary().unwrap().get("About Will"),
        Some(&Value::Boolean(true))
    );

    let mut expected = Dictionary::new();
    expected.insert("text".into(), "Café 火 😀 <&>".into());
    expected.insert("seen".into(), true.into());
    expected.insert("integer".into(), (-42i64).into());
    expected.insert("real".into(), 0.625f64.into());
    expected.insert("bytes".into(), Value::Data(vec![0, 1, 0, 255]));
    expected.insert("empty".into(), Value::Data(vec![]));
    expected.insert(
        "date".into(),
        Value::Date(plist::Date::from_xml_format("2000-01-01T00:00:00Z").unwrap()),
    );
    expected.insert(
        "items".into(),
        Value::Array(vec!["one".into(), "two".into()]),
    );
    let expected = Value::Dictionary(expected);
    let mut input = Vec::new();
    expected.to_writer_xml(&mut input).unwrap();
    let data = data_from_bytes(env, &input);
    let root: id = msg_class![env; NSPropertyListSerialization propertyListFromData:data
        mutabilityOption:NSPropertyListMutableContainersAndLeaves format:format_ptr errorDescription:error_ptr];
    assert_ne!(root, nil);
    assert_eq!(env.mem.read(format_ptr), NSPropertyListXMLFormat_v1_0);
    let mutable = env
        .objc
        .get_known_class("NSMutableDictionary", &mut env.mem);
    assert!(msg![env; root isKindOfClass:mutable]);
    let key = get_static_str(env, "bytes");
    let leaf: id = msg![env; root objectForKey:key];
    let mutable = env.objc.get_known_class("NSMutableData", &mut env.mem);
    assert!(msg![env; leaf isKindOfClass:mutable]);
    for format in [
        NSPropertyListXMLFormat_v1_0,
        NSPropertyListBinaryFormat_v1_0,
    ] {
        let encoded: id = msg_class![env; NSPropertyListSerialization dataFromPropertyList:root
            format:format errorDescription:error_ptr];
        assert_ne!(encoded, nil);
        let bytes = crate::frameworks::foundation::ns_data::to_rust_slice(env, encoded);
        let actual = Value::from_reader(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(actual, expected);
        let decoded: id = msg_class![env; NSPropertyListSerialization propertyListFromData:encoded
            mutabilityOption:NSPropertyListImmutable format:format_ptr errorDescription:error_ptr];
        assert_ne!(decoded, nil);
        assert_eq!(env.mem.read(format_ptr), format);
    }
    // Repeated acyclic containers are legal; self-references are not.
    let array: id = msg_class![env; NSMutableArray array];
    () = msg![env; array addObject:root];
    () = msg![env; array addObject:root];
    let encoded: id = msg_class![env; NSPropertyListSerialization dataFromPropertyList:array
        format:NSPropertyListXMLFormat_v1_0 errorDescription:error_ptr];
    assert_ne!(encoded, nil);
    let bytes = crate::frameworks::foundation::ns_data::to_rust_slice(env, encoded);
    let value = Value::from_reader(std::io::Cursor::new(bytes)).unwrap();
    assert_eq!(
        value.as_array().unwrap(),
        &[expected.clone(), expected.clone()]
    );
    () = msg![env; array addObject:array];
    let failed: id = msg_class![env; NSPropertyListSerialization dataFromPropertyList:array
        format:NSPropertyListXMLFormat_v1_0 errorDescription:error_ptr];
    assert_eq!(failed, nil);
    let error = env.mem.read(error_ptr);
    assert_ne!(error, nil);
    release(env, error);
    () = msg![env; array removeLastObject];
    for (object, format) in [
        (root, 999u32),
        (msg_class![env; NSNull null], NSPropertyListXMLFormat_v1_0),
    ] {
        let failed: id = msg_class![env; NSPropertyListSerialization dataFromPropertyList:object
            format:format errorDescription:error_ptr];
        assert_eq!(failed, nil);
        release(env, env.mem.read(error_ptr));
    }
    if let Some(directory) = std::env::var_os("TOUCHHLE_PLIST_FIXTURES") {
        for name in ["native_plist.xml", "native_plist.bin"] {
            let bytes = std::fs::read(std::path::Path::new(&directory).join(name)).unwrap();
            let native = Value::from_reader(std::io::Cursor::new(&bytes)).unwrap();
            assert_eq!(native, expected);
            let data = data_from_bytes(env, &bytes);
            let root: id = msg_class![env; NSPropertyListSerialization propertyListFromData:data
                mutabilityOption:NSPropertyListMutableContainersAndLeaves format:format_ptr errorDescription:error_ptr];
            let encoded: id = msg_class![env; NSPropertyListSerialization dataFromPropertyList:root
                format:NSPropertyListXMLFormat_v1_0 errorDescription:error_ptr];
            let bytes = crate::frameworks::foundation::ns_data::to_rust_slice(env, encoded);
            assert_eq!(
                Value::from_reader_xml(std::io::Cursor::new(bytes)).unwrap(),
                expected
            );
        }
    }
    env.mem.free(error_ptr.cast());
    env.mem.free(format_ptr.cast());
}

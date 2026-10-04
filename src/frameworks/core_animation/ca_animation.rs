/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `CAAnimation` and its subclasses

use std::collections::HashMap;

use crate::dyld::{ConstantExports, HostConstant};
use crate::frameworks::core_animation::ca_media_timing_function::kCAMediaTimingFunctionDefault;
use crate::frameworks::core_foundation::time::CFTimeInterval;
use crate::frameworks::foundation::ns_string::{get_static_str, to_rust_string};
use crate::objc::{
    autorelease, id, msg, msg_send, nil, objc_classes, release, retain, todo_objc_setter,
    ClassExports, HostObject, NSZonePtr,
};
use crate::Environment;
use crate::{impl_HostObject_with_superclass, msg_class, msg_super};

type CATransitionType = id; // NSString*
const kCATransitionFade: &str = "fade";
const kCATransitionMoveIn: &str = "moveIn";
const kCATransitionPush: &str = "push";
const kCATransitionReveal: &str = "reveal";

pub type CAMediaTimingFillMode = id; // NSString*
pub const kCAFillModeBackwards: &str = "backwards";
pub const kCAFillModeBoth: &str = "both";
pub const kCAFillModeForwards: &str = "forwards";
pub const kCAFillModeRemoved: &str = "removed";

pub const CONSTANTS: ConstantExports = &[
    // `CATransitionType` values.
    (
        "_kCATransitionFade",
        HostConstant::NSString(kCATransitionFade),
    ),
    (
        "_kCATransitionMoveIn",
        HostConstant::NSString(kCATransitionMoveIn),
    ),
    (
        "_kCATransitionPush",
        HostConstant::NSString(kCATransitionPush),
    ),
    (
        "_kCATransitionReveal",
        HostConstant::NSString(kCATransitionReveal),
    ),
    // `CAMediaTimingFillMode` values.
    (
        "_kCAFillModeBackwards",
        HostConstant::NSString(kCAFillModeBackwards),
    ),
    ("_kCAFillModeBoth", HostConstant::NSString(kCAFillModeBoth)),
    (
        "_kCAFillModeForwards",
        HostConstant::NSString(kCAFillModeForwards),
    ),
    (
        "_kCAFillModeRemoved",
        HostConstant::NSString(kCAFillModeRemoved),
    ),
];

#[derive(Clone)]
struct CAAnimationHostObject {
    removed_on_completion: bool,
    timing_function: id, // CAMediaTimingFunction*
    delegate: id,        // CAAnimationDelegate*
    autoreverses: bool,
    repeat_count: f32,
    begin_time: CFTimeInterval,
    duration: CFTimeInterval,
    repeat_duration: CFTimeInterval,
    speed: f32,
    time_offset: CFTimeInterval,
    fill_mode: &'static str,
    started_at: Option<CFTimeInterval>,
    pub(super) completion_reported: bool,
    /// Arbitrary KVC values travel with animation copies and callbacks.
    metadata: HashMap<String, id>,
}
impl HostObject for CAAnimationHostObject {}
impl Default for CAAnimationHostObject {
    fn default() -> Self {
        Self {
            removed_on_completion: true,
            timing_function: Default::default(),
            delegate: Default::default(),
            autoreverses: Default::default(),
            repeat_count: Default::default(),
            begin_time: Default::default(),
            duration: Default::default(),
            repeat_duration: 0.0,
            speed: 1.0,
            time_offset: 0.0,
            fill_mode: kCAFillModeRemoved,
            started_at: None,
            completion_reported: false,
            metadata: HashMap::new(),
        }
    }
}

#[derive(Default, Clone)]
struct CAPropertyAnimationHostObject {
    superclass: CAAnimationHostObject,
    key_path: id, // NSString*
}
impl_HostObject_with_superclass!(CAPropertyAnimationHostObject);

#[derive(Default, Clone)]
struct CABasicAnimationHostObject {
    superclass: CAPropertyAnimationHostObject,
    from_value: id,
    to_value: id,
    by_value: id,
}
impl_HostObject_with_superclass!(CABasicAnimationHostObject);

#[derive(Default, Clone)]
struct CAKeyframeAnimationHostObject {
    superclass: CAPropertyAnimationHostObject,
    values: id,
    key_times: id,
    timing_functions: id,
    path: id,
    calculation_mode: String,
}
impl_HostObject_with_superclass!(CAKeyframeAnimationHostObject);

#[derive(Default, Clone)]
struct CAAnimationGroupHostObject {
    superclass: CAAnimationHostObject,
    animations: id,
}
impl_HostObject_with_superclass!(CAAnimationGroupHostObject);

pub const kCAAnimationLinear: &str = "linear";
pub const kCAAnimationDiscrete: &str = "discrete";
pub const KEYFRAME_CONSTANTS: ConstantExports = &[
    (
        "_kCAAnimationLinear",
        HostConstant::NSString(kCAAnimationLinear),
    ),
    (
        "_kCAAnimationDiscrete",
        HostConstant::NSString(kCAAnimationDiscrete),
    ),
];

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

// CAAnimation is an abstract class.
@implementation CAAnimation: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<CAAnimationHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

+ (id)animation {
    let object = msg![env; this new];
    autorelease(env, object)
}

- (id)init {
    let default_timing_function_name: id = get_static_str(env, kCAMediaTimingFunctionDefault);
    let default_timing_function: id = msg_class![env; CAMediaTimingFunction functionWithName: default_timing_function_name];
    () = msg![env; this setTimingFunction: default_timing_function];
    this
}

- (())setValue:(id)value forKey:(id)key {
    let key = to_rust_string(env, key).into_owned();
    let first = key.chars().next().expect("KVC key must not be empty");
    let setter = format!("set{}{}:", first.to_uppercase(), &key[first.len_utf8()..]);
    let selector = env.objc.register_host_selector(setter, &mut env.mem);
    if env.objc.object_has_method(&env.mem, this, selector) {
        match key.as_str() {
            "removedOnCompletion" | "autoreverses" => {
                assert!(value != nil);
                let value: bool = msg![env; value boolValue];
                () = msg_send(env, (this, selector, value));
            }
            "beginTime" | "duration" | "repeatDuration" | "timeOffset" => {
                assert!(value != nil);
                let value: f64 = msg![env; value doubleValue];
                () = msg_send(env, (this, selector, value));
            }
            "repeatCount" | "speed" => {
                assert!(value != nil);
                let value: f32 = msg![env; value floatValue];
                () = msg_send(env, (this, selector, value));
            }
            _ => { () = msg_send(env, (this, selector, value)); }
        }
        return;
    }
    retain(env, value);
    let metadata = &mut env.objc.borrow_mut::<CAAnimationHostObject>(this).metadata;
    let old = if value == nil {
        metadata.remove(&key)
    } else {
        metadata.insert(key, value)
    };
    if let Some(old) = old {
        release(env, old);
    }
}

- (id)valueForKey:(id)key {
    let key = to_rust_string(env, key).into_owned();
    let getter = if key == "removedOnCompletion" { "isRemovedOnCompletion" } else { &key };
    let selector = env.objc.register_host_selector(getter.into(), &mut env.mem);
    if env.objc.object_has_method(&env.mem, this, selector) {
        match key.as_str() {
            "removedOnCompletion" | "autoreverses" => {
                let value: bool = msg_send(env, (this, selector));
                return msg_class![env; NSNumber numberWithBool:value];
            }
            "beginTime" | "duration" | "repeatDuration" | "timeOffset" => {
                let value: f64 = msg_send(env, (this, selector));
                return msg_class![env; NSNumber numberWithDouble:value];
            }
            "repeatCount" | "speed" => {
                let value: f32 = msg_send(env, (this, selector));
                return msg_class![env; NSNumber numberWithFloat:value];
            }
            "delegate" | "timingFunction" | "fillMode" | "keyPath" |
            "fromValue" | "toValue" | "byValue" | "values" | "keyTimes" |
            "timingFunctions" | "path" | "calculationMode" | "animations" => {
                return msg_send(env, (this, selector));
            }
            _ => {}
        }
    }
    env.objc.borrow::<CAAnimationHostObject>(this).metadata.get(&key).copied().unwrap_or(nil)
}

- (())setRemovedOnCompletion:(bool)removed_on_completion {
    log_dbg!("[(CAAnimation*){:?} setRemovedOnCompletion:{:?}]", this, removed_on_completion);
    env.objc.borrow_mut::<CAAnimationHostObject>(this).removed_on_completion = removed_on_completion;
}
- (bool)isRemovedOnCompletion {
    env.objc.borrow::<CAAnimationHostObject>(this).removed_on_completion
}

- (())setDelegate:(id)delegate { // CAAnimationDelegate*
    log_dbg!("[(CAAnimation*){:?} setDelegate:{:?}]", this, delegate);
    retain(env, delegate);
    let old_delegate = std::mem::replace(&mut env.objc.borrow_mut::<CAAnimationHostObject>(this).delegate, delegate);
    release(env, old_delegate);
}
- (id)delegate {
    env.objc.borrow::<CAAnimationHostObject>(this).delegate
}

- (())setTimingFunction:(id)timingFunction { // CAMediaTimingFunction*
    log_dbg!("[(CAAnimation*){:?} setTimingFunction:{:?}]", this, timingFunction);
    retain(env, timingFunction);
    let old_function = std::mem::replace(&mut env.objc.borrow_mut::<CAAnimationHostObject>(this).timing_function, timingFunction);
    release(env, old_function);
}
- (id)timingFunction {
    env.objc.borrow::<CAAnimationHostObject>(this).timing_function
}

// CAMediaTiming protocol implementation
- (())setAutoreverses:(bool)autoreverses {
    log_dbg!("[(CAAnimation*){:?} setAutoreverses:{:?}]", this, autoreverses);
    env.objc.borrow_mut::<CAAnimationHostObject>(this).autoreverses = autoreverses;
}
- (bool)autoreverses {
    env.objc.borrow::<CAAnimationHostObject>(this).autoreverses
}

- (())setRepeatCount:(f32)repeatCount {
    log_dbg!("[(CAAnimation*){:?} setRepeatCount:{:?}]", this, repeatCount);
    env.objc.borrow_mut::<CAAnimationHostObject>(this).repeat_count = repeatCount;
}
- (f32)repeatCount {
    env.objc.borrow::<CAAnimationHostObject>(this).repeat_count
}

- (())setBeginTime:(CFTimeInterval)beginTime {
    log_dbg!("[(CAAnimation*){:?} setBeginTime:{:?}]", this, beginTime);
    env.objc.borrow_mut::<CAAnimationHostObject>(this).begin_time = beginTime;
}
- (CFTimeInterval)beginTime {
    env.objc.borrow::<CAAnimationHostObject>(this).begin_time
}

- (())setDuration:(CFTimeInterval)duration {
    log_dbg!("[(CAAnimation*){:?} setDuration:{:?}]", this, duration);
    env.objc.borrow_mut::<CAAnimationHostObject>(this).duration = duration;
}
- (CFTimeInterval)duration {
    env.objc.borrow::<CAAnimationHostObject>(this).duration
}

- (())setRepeatDuration:(CFTimeInterval)value {
    env.objc
        .borrow_mut::<CAAnimationHostObject>(this)
        .repeat_duration = value;
}
- (CFTimeInterval)repeatDuration {
    env.objc
        .borrow::<CAAnimationHostObject>(this)
        .repeat_duration
}
- (())setSpeed:(f32)value {
    env.objc.borrow_mut::<CAAnimationHostObject>(this).speed = value;
}
- (f32)speed {
    env.objc.borrow::<CAAnimationHostObject>(this).speed
}
- (())setTimeOffset:(CFTimeInterval)value {
    env.objc
        .borrow_mut::<CAAnimationHostObject>(this)
        .time_offset = value;
}
- (CFTimeInterval)timeOffset {
    env.objc.borrow::<CAAnimationHostObject>(this).time_offset
}

- (id)copyWithZone:(NSZonePtr)_zone {
    copy_animation(env, this)
}

- (())setFillMode:(CAMediaTimingFillMode)fill_mode {
    let fill_mode_str = to_rust_string(env, fill_mode);
    log_dbg!(
        "[(CAAnimation*){:?} setFillMode:{:?} ({})]",
        this,
        fill_mode,
        fill_mode_str
    );
    let fill_mode_str = match &*fill_mode_str {
        kCAFillModeBackwards => kCAFillModeBackwards,
        kCAFillModeBoth => kCAFillModeBoth,
        kCAFillModeForwards => kCAFillModeForwards,
        kCAFillModeRemoved => kCAFillModeRemoved,
        _ => panic!("Unknown fill mode \"{}\"", fill_mode_str),
    };
    env.objc.borrow_mut::<CAAnimationHostObject>(this).fill_mode = fill_mode_str;
}
- (CAMediaTimingFillMode)fillMode {
    let fill_mode = env.objc.borrow::<CAAnimationHostObject>(this).fill_mode;
    get_static_str(env, fill_mode)
}

- (())dealloc {
    let &CAAnimationHostObject { delegate, timing_function, .. } = env.objc.borrow(this);
    if delegate != nil {
        release(env, delegate);
    }
    if timing_function != nil {
        release(env, timing_function);
    }
    let metadata = std::mem::take(&mut env.objc.borrow_mut::<CAAnimationHostObject>(this).metadata);
    for value in metadata.into_values() {
        release(env, value);
    }

    env.objc.dealloc_object(this, &mut env.mem)
}

@end


@implementation CAPropertyAnimation: CAAnimation

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<CAPropertyAnimationHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

+ (id)animationWithKeyPath:(id)path { // NSString*
    let object = msg![env; this new];
    log_dbg!("[CAPropertyAnimation animationWithKeyPath:{:?} ({:?})] -> {:?}", path, to_rust_string(env, path), object);
    () = msg![env; object setKeyPath:path];
    autorelease(env, object)
}

- (())setKeyPath:(id)path {
    // NSString*
    log_dbg!(
        "[(CAPropertyAnimation*){:?} setKeyPath:{:?} ({:?})]",
        this,
        path,
        to_rust_string(env, path)
    );
    let path_copy: id = msg![env; path copy];
    let old = std::mem::replace(
        &mut env
            .objc
            .borrow_mut::<CAPropertyAnimationHostObject>(this)
            .key_path,
        path_copy,
    );
    release(env, old);
}
- (id)keyPath {
    env.objc.borrow::<CAPropertyAnimationHostObject>(this).key_path
}

- (())dealloc {
    let &CAPropertyAnimationHostObject { key_path, .. } = env.objc.borrow(this);
    if key_path != nil {
        release(env, key_path);
    }

    msg_super![env; this dealloc]
}

@end


@implementation CABasicAnimation: CAPropertyAnimation

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<CABasicAnimationHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (())setFromValue:(id)value {
    log_dbg!("[(CABasicAnimation*){:?} setFromValue:{:?}]", this, value);
    retain(env, value);
    let old = std::mem::replace(
        &mut env
            .objc
            .borrow_mut::<CABasicAnimationHostObject>(this)
            .from_value,
        value,
    );
    release(env, old);
}
- (id)fromValue {
    env.objc.borrow::<CABasicAnimationHostObject>(this).from_value
}

- (())setToValue:(id)value {
    log_dbg!("[(CABasicAnimation*){:?} setToValue:{:?}]", this, value);
    retain(env, value);
    let old = std::mem::replace(
        &mut env
            .objc
            .borrow_mut::<CABasicAnimationHostObject>(this)
            .to_value,
        value,
    );
    release(env, old);
}
- (id)toValue {
    env.objc.borrow::<CABasicAnimationHostObject>(this).to_value
}

- (())setByValue:(id)value {
    log_dbg!("[(CABasicAnimation*){:?} setByValue:{:?}]", this, value);
    retain(env, value);
    let old = std::mem::replace(
        &mut env
            .objc
            .borrow_mut::<CABasicAnimationHostObject>(this)
            .by_value,
        value,
    );
    release(env, old);
}
- (id)byValue {
    env.objc.borrow::<CABasicAnimationHostObject>(this).by_value
}

- (())dealloc {
    let &CABasicAnimationHostObject {
        from_value,
        to_value,
        by_value,
        ..
    } = env.objc.borrow(this);
    release(env, by_value);
    if from_value != nil {
        release(env, from_value);
    }
    if to_value != nil {
        release(env, to_value);
    }

    msg_super![env; this dealloc]
}

@end


@implementation CAKeyframeAnimation: CAPropertyAnimation

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host = Box::<CAKeyframeAnimationHostObject>::default();
    env.objc.alloc_object(this, host, &mut env.mem)
}
- (id)values {
    env.objc
        .borrow::<CAKeyframeAnimationHostObject>(this)
        .values
}
- (())setValues:(id)value {
    let value: id = msg![env; value copy];
    let old = std::mem::replace(
        &mut env
            .objc
            .borrow_mut::<CAKeyframeAnimationHostObject>(this)
            .values,
        value,
    );
    release(env, old);
}
- (id)keyTimes {
    env.objc
        .borrow::<CAKeyframeAnimationHostObject>(this)
        .key_times
}
- (())setKeyTimes:(id)value {
    let value: id = msg![env; value copy];
    let old = std::mem::replace(
        &mut env
            .objc
            .borrow_mut::<CAKeyframeAnimationHostObject>(this)
            .key_times,
        value,
    );
    release(env, old);
}
- (id)timingFunctions {
    env.objc
        .borrow::<CAKeyframeAnimationHostObject>(this)
        .timing_functions
}
- (())setTimingFunctions:(id)value {
    let value: id = msg![env; value copy];
    let old = std::mem::replace(
        &mut env
            .objc
            .borrow_mut::<CAKeyframeAnimationHostObject>(this)
            .timing_functions,
        value,
    );
    release(env, old);
}
- (id)path {
    env.objc.borrow::<CAKeyframeAnimationHostObject>(this).path
}
- (())setPath:(id)value {
    retain(env, value);
    let old = std::mem::replace(
        &mut env
            .objc
            .borrow_mut::<CAKeyframeAnimationHostObject>(this)
            .path,
        value,
    );
    release(env, old);
}
- (id)calculationMode {
    let value = env
        .objc
        .borrow::<CAKeyframeAnimationHostObject>(this)
        .calculation_mode
        .clone();
    get_static_str(
        env,
        if value.is_empty() {
            kCAAnimationLinear
        } else if value == kCAAnimationDiscrete {
            kCAAnimationDiscrete
        } else {
            kCAAnimationLinear
        },
    )
}
- (())setCalculationMode:(id)value {
    let value = to_rust_string(env, value).to_string();
    assert!(
        value == kCAAnimationLinear || value == kCAAnimationDiscrete,
        "Unsupported keyframe calculation mode: {value}"
    );
    env.objc
        .borrow_mut::<CAKeyframeAnimationHostObject>(this)
        .calculation_mode = value;
}
- (())dealloc {
    let host = env.objc.borrow::<CAKeyframeAnimationHostObject>(this);
    let refs = [
        host.values,
        host.key_times,
        host.timing_functions,
        host.path,
    ];
    for value in refs {
        release(env, value);
    }
    msg_super![env; this dealloc]
}
@end

@implementation CAAnimationGroup: CAAnimation
+ (id)allocWithZone:(NSZonePtr)_zone {
    let host = Box::<CAAnimationGroupHostObject>::default();
    env.objc.alloc_object(this, host, &mut env.mem)
}
- (id)animations {
    env.objc
        .borrow::<CAAnimationGroupHostObject>(this)
        .animations
}
- (())setAnimations:(id)value {
    let value: id = msg![env; value copy];
    let old = std::mem::replace(
        &mut env
            .objc
            .borrow_mut::<CAAnimationGroupHostObject>(this)
            .animations,
        value,
    );
    release(env, old);
}
- (())dealloc {
    let value = env
        .objc
        .borrow::<CAAnimationGroupHostObject>(this)
        .animations;
    release(env, value);
    msg_super![env; this dealloc]
}
@end


@implementation CATransition : CAAnimation

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<CABasicAnimationHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (())setType:(CATransitionType)transitionType {
    todo_objc_setter!(this, to_rust_string(env, transitionType));
}

@end

};

pub fn get_animation_start_time(
    env: &mut Environment,
    animation: id,
) -> &mut Option<CFTimeInterval> {
    &mut env
        .objc
        .borrow_mut::<CAAnimationHostObject>(animation)
        .started_at
}

pub(super) fn mark_completion(env: &mut Environment, animation: id) -> bool {
    !std::mem::replace(
        &mut env
            .objc
            .borrow_mut::<CAAnimationHostObject>(animation)
            .completion_reported,
        true,
    )
}

fn copy_animation(env: &mut Environment, animation: id) -> id {
    let class: crate::objc::Class = msg![env; animation class];
    let mut refs = Vec::new();
    let group = env.objc.get_known_class("CAAnimationGroup", &mut env.mem);
    let keyframe = env
        .objc
        .get_known_class("CAKeyframeAnimation", &mut env.mem);
    let basic = env.objc.get_known_class("CABasicAnimation", &mut env.mem);
    let host: Box<dyn crate::objc::AnyHostObject> = if env.objc.class_is_subclass_of(class, group) {
        let mut value = env
            .objc
            .borrow::<CAAnimationGroupHostObject>(animation)
            .clone();
        if value.animations != nil {
            let count: u32 = msg![env; (value.animations) count];
            let children: Vec<_> = (0..count)
                .map(|i| {
                    let child: id = msg![env; (value.animations) objectAtIndex:i];
                    msg![env; child copy]
                })
                .collect();
            value.animations = crate::frameworks::foundation::ns_array::from_vec(env, children);
        }
        Box::new(value)
    } else if env.objc.class_is_subclass_of(class, keyframe) {
        let value = env
            .objc
            .borrow::<CAKeyframeAnimationHostObject>(animation)
            .clone();
        refs.extend([
            value.values,
            value.key_times,
            value.timing_functions,
            value.path,
            value.superclass.key_path,
        ]);
        Box::new(value)
    } else if env.objc.class_is_subclass_of(class, basic) {
        let value = env
            .objc
            .borrow::<CABasicAnimationHostObject>(animation)
            .clone();
        refs.extend([
            value.from_value,
            value.to_value,
            value.by_value,
            value.superclass.key_path,
        ]);
        Box::new(value)
    } else {
        Box::new(env.objc.borrow::<CAAnimationHostObject>(animation).clone())
    };
    let parent = env.objc.borrow::<CAAnimationHostObject>(animation);
    refs.extend([parent.delegate, parent.timing_function]);
    refs.extend(parent.metadata.values().copied());
    for value in refs {
        retain(env, value);
    }
    let copy = env.objc.alloc_object(class, host, &mut env.mem);
    let parent = env.objc.borrow_mut::<CAAnimationHostObject>(copy);
    parent.started_at = None;
    parent.completion_reported = false;
    copy
}

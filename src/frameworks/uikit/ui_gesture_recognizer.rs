/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Minimal UIKit gesture recognizers for applications with gesture-driven input.
//! The view retains its recognizers; delegates and action targets are weak.

use crate::frameworks::core_graphics::CGPoint;
use crate::frameworks::foundation::{NSInteger, NSTimeInterval, NSUInteger};
use crate::objc::{
    id, msg, msg_send, nil, objc_classes, release, retain, ClassExports, HostObject, NSZonePtr,
    SEL,
};
use crate::Environment;

const POSSIBLE: NSInteger = 0;
const BEGAN: NSInteger = 1;
const CHANGED: NSInteger = 2;
const ENDED: NSInteger = 3;
const CANCELLED: NSInteger = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Tap,
    LongPress,
    Pan,
    Pinch,
    Rotation,
    Other,
}

struct GestureHostObject {
    kind: Kind,
    /// UIView (non-retaining to avoid a retain cycle).
    view: id,
    /// Delegate and target/action recipients are non-retaining.
    delegate: id,
    targets: Vec<(id, SEL)>,
    state: NSInteger,
    enabled: bool,
    cancels_touches_in_view: bool,
    number_of_taps_required: NSUInteger,
    number_of_touches_required: NSUInteger,
    minimum_press_duration: f64,
    start: CGPoint,
    last: CGPoint,
    start_time: NSTimeInterval,
    active: bool,
    moved: bool,
    translation: CGPoint,
    velocity: CGPoint,
    scale: f32,
    rotation: f32,
}
impl HostObject for GestureHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIGestureRecognizer: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let kind = match env.objc.get_class_name(this) {
        "UITapGestureRecognizer" => Kind::Tap,
        "UILongPressGestureRecognizer" => Kind::LongPress,
        "UIPanGestureRecognizer" => Kind::Pan,
        "UIPinchGestureRecognizer" => Kind::Pinch,
        "UIRotationGestureRecognizer" => Kind::Rotation,
        _ => Kind::Other,
    };
    env.objc.alloc_object(this, Box::new(GestureHostObject {
        kind, view: nil, delegate: nil, targets: Vec::new(), state: POSSIBLE,
        enabled: true, cancels_touches_in_view: true,
        number_of_taps_required: 1, number_of_touches_required: 1,
        minimum_press_duration: 0.5,
        start: CGPoint { x: 0.0, y: 0.0 },
        last: CGPoint { x: 0.0, y: 0.0 },
        start_time: 0.0, active: false, moved: false,
        translation: CGPoint { x: 0.0, y: 0.0 },
        velocity: CGPoint { x: 0.0, y: 0.0 },
        scale: 1.0, rotation: 0.0,
    }), &mut env.mem)
}

- (id)initWithTarget:(id)target action:(SEL)action {
    if target != nil {
        env.objc.borrow_mut::<GestureHostObject>(this).targets.push((target, action));
    }
    this
}

- (())addTarget:(id)target action:(SEL)action {
    if target != nil {
        env.objc.borrow_mut::<GestureHostObject>(this).targets.push((target, action));
    }
}

- (())removeTarget:(id)target action:(SEL)action {
    env.objc.borrow_mut::<GestureHostObject>(this).targets.retain(|&(t, a)| {
        !(t == target && a == action)
    });
}

- (())dealloc {
    env.objc.dealloc_object(this, &mut env.mem);
}

- (id)view { env.objc.borrow::<GestureHostObject>(this).view }
- (id)delegate { env.objc.borrow::<GestureHostObject>(this).delegate }
- (())setDelegate:(id)delegate { env.objc.borrow_mut::<GestureHostObject>(this).delegate = delegate; }
- (NSInteger)state { env.objc.borrow::<GestureHostObject>(this).state }
- (bool)isEnabled { env.objc.borrow::<GestureHostObject>(this).enabled }
- (bool)enabled { env.objc.borrow::<GestureHostObject>(this).enabled }
- (())setEnabled:(bool)value {
    let recognizer = env.objc.borrow_mut::<GestureHostObject>(this);
    recognizer.enabled = value;
    if !value { recognizer.active = false; recognizer.state = POSSIBLE; }
}
- (bool)cancelsTouchesInView { env.objc.borrow::<GestureHostObject>(this).cancels_touches_in_view }
- (())setCancelsTouchesInView:(bool)value {
    env.objc.borrow_mut::<GestureHostObject>(this).cancels_touches_in_view = value;
}
- (())setDelaysTouchesBegan:(bool)_value {}
- (())setDelaysTouchesEnded:(bool)_value {}
- (())requireGestureRecognizerToFail:(id)_other {}
- (())reset { env.objc.borrow_mut::<GestureHostObject>(this).state = POSSIBLE; }
- (NSUInteger)numberOfTapsRequired { env.objc.borrow::<GestureHostObject>(this).number_of_taps_required }
- (())setNumberOfTapsRequired:(NSUInteger)value {
    env.objc.borrow_mut::<GestureHostObject>(this).number_of_taps_required = value;
}
- (NSUInteger)numberOfTouchesRequired { env.objc.borrow::<GestureHostObject>(this).number_of_touches_required }
- (())setNumberOfTouchesRequired:(NSUInteger)value {
    env.objc.borrow_mut::<GestureHostObject>(this).number_of_touches_required = value;
}
- (NSTimeInterval)minimumPressDuration { env.objc.borrow::<GestureHostObject>(this).minimum_press_duration }
- (())setMinimumPressDuration:(NSTimeInterval)value {
    env.objc.borrow_mut::<GestureHostObject>(this).minimum_press_duration = value;
}
- (NSUInteger)numberOfTouches {
    if env.objc.borrow::<GestureHostObject>(this).active { 1 } else { 0 }
}
- (CGPoint)locationInView:(id)view {
    let (location, own_view) = {
        let g = env.objc.borrow::<GestureHostObject>(this);
        (g.last, g.view)
    };
    if own_view == nil || view == own_view { return location; }
    msg![env; own_view convertPoint:location toView:view]
}
- (CGPoint)translationInView:(id)_view {
    env.objc.borrow::<GestureHostObject>(this).translation
}
- (())setTranslation:(CGPoint)translation inView:(id)_view {
    env.objc.borrow_mut::<GestureHostObject>(this).translation = translation;
}
- (CGPoint)velocityInView:(id)_view { env.objc.borrow::<GestureHostObject>(this).velocity }
- (f32)scale { env.objc.borrow::<GestureHostObject>(this).scale }
- (())setScale:(f32)scale { env.objc.borrow_mut::<GestureHostObject>(this).scale = scale; }
- (f32)rotation { env.objc.borrow::<GestureHostObject>(this).rotation }
- (())setRotation:(f32)rotation { env.objc.borrow_mut::<GestureHostObject>(this).rotation = rotation; }

@end

@implementation UITapGestureRecognizer: UIGestureRecognizer
@end
@implementation UILongPressGestureRecognizer: UIGestureRecognizer
@end
@implementation UIPanGestureRecognizer: UIGestureRecognizer
@end
@implementation UIPinchGestureRecognizer: UIGestureRecognizer
@end
@implementation UIRotationGestureRecognizer: UIGestureRecognizer
@end

};

/// A UIView owns its attached recognizers; the recognizers reference it weakly.
pub(crate) fn attach(env: &mut Environment, recognizer: id, view: id) {
    env.objc.borrow_mut::<GestureHostObject>(recognizer).view = view;
}

pub(crate) fn detach(env: &mut Environment, recognizer: id) {
    env.objc.borrow_mut::<GestureHostObject>(recognizer).view = nil;
}

fn fire(env: &mut Environment, recognizer: id) {
    let targets = env.objc.borrow::<GestureHostObject>(recognizer).targets.clone();
    retain(env, recognizer);
    for (target, action) in targets {
        if target == nil { continue; }
        let arity = action.as_str(&env.mem).bytes().filter(|&b| b == b':').count();
        match arity {
            0 => { () = msg_send(env, (target, action)); }
            1 => { () = msg_send(env, (target, action, recognizer)); }
            _ => log!("Ignoring gesture action with unsupported arity: {:?}", action),
        }
    }
    release(env, recognizer);
}

/// Called alongside regular UIWindow touch dispatch. Only attached gestures
/// receive touch notifications. Touches still reach their UIView normally.
pub(crate) fn dispatch(
    env: &mut Environment,
    recognizer: id,
    touch: id,
    phase: NSInteger,
) {
    let (view, enabled, delegate, kind) = {
        let g = env.objc.borrow::<GestureHostObject>(recognizer);
        (g.view, g.enabled, g.delegate, g.kind)
    };
    if view == nil || !enabled { return; }

    if phase == 0 && delegate != nil {
        let sel = env.objc.lookup_selector("gestureRecognizer:shouldReceiveTouch:").unwrap();
        let responds: bool = msg![env; delegate respondsToSelector:sel];
        if responds {
            let allowed: bool = msg_send(env, (delegate, sel, recognizer, touch));
            if !allowed { return; }
        }
    }

    let location: CGPoint = msg![env; touch locationInView:view];
    let time: NSTimeInterval = msg![env; touch timestamp];
    let mut should_fire = false;
    {
        let g = env.objc.borrow_mut::<GestureHostObject>(recognizer);
        match phase {
            0 => {
                g.start = location;
                g.last = location;
                g.start_time = time;
                g.translation = CGPoint { x: 0.0, y: 0.0 };
                g.velocity = CGPoint { x: 0.0, y: 0.0 };
                g.moved = false;
                g.active = true;
                g.state = POSSIBLE;
            }
            1 if g.active => {
                let dx = location.x - g.start.x;
                let dy = location.y - g.start.y;
                let delta_x = location.x - g.last.x;
                let delta_y = location.y - g.last.y;
                g.last = location;
                g.translation = CGPoint { x: dx, y: dy };
                g.velocity = CGPoint { x: delta_x * 60.0, y: delta_y * 60.0 };
                if dx * dx + dy * dy > 36.0 { g.moved = true; }
                if kind == Kind::Pan && g.moved {
                    g.state = if g.state == POSSIBLE { BEGAN } else { CHANGED };
                    should_fire = true;
                } else if kind == Kind::LongPress && !g.moved
                    && time - g.start_time >= g.minimum_press_duration
                {
                    g.state = BEGAN;
                    should_fire = true;
                }
            }
            3 if g.active => {
                let dx = location.x - g.start.x;
                let dy = location.y - g.start.y;
                g.last = location;
                g.translation = CGPoint { x: dx, y: dy };
                g.active = false;
                match kind {
                    Kind::Tap if !g.moved && dx * dx + dy * dy <= 144.0
                        && g.number_of_taps_required == 1
                        && g.number_of_touches_required == 1 =>
                    {
                        g.state = ENDED;
                        should_fire = true;
                    }
                    Kind::Pan if g.state == BEGAN || g.state == CHANGED => {
                        g.state = ENDED;
                        should_fire = true;
                    }
                    Kind::LongPress if time - g.start_time >= g.minimum_press_duration
                        && !g.moved =>
                    {
                        g.state = ENDED;
                        should_fire = true;
                    }
                    _ => { g.state = POSSIBLE; }
                }
            }
            4 if g.active => {
                g.active = false;
                g.state = CANCELLED;
                if kind == Kind::Pan { should_fire = true; }
            }
            _ => {}
        }
    }

    if should_fire {
        if delegate != nil {
            let sel = env.objc.lookup_selector("gestureRecognizerShouldBegin:").unwrap();
            let responds: bool = msg![env; delegate respondsToSelector:sel];
            if responds {
                let allowed: bool = msg_send(env, (delegate, sel, recognizer));
                if !allowed { return; }
            }
        }
        fire(env, recognizer);
    }
}

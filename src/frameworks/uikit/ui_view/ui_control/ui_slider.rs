/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Slider values, custom images, rendering and touch tracking.
use super::{UIControlEventValueChanged, UIControlHostObject, UIControlStateNormal};
use crate::frameworks::core_graphics::{CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::ns_string::get_static_str;
use crate::objc::{
    id, msg, msg_super, nil, objc_classes, release, retain, ClassExports, NSZonePtr,
};
use crate::{impl_HostObject_with_superclass, Environment};
use std::collections::HashMap;
#[derive(Default)]
struct SliderHostObject {
    superclass: UIControlHostObject,
    value: f32,
    minimum: f32,
    maximum: f32,
    tint: id,
    minimum_image: id,
    maximum_image: id,
    images: HashMap<(u8, u32), id>,
}
impl_HostObject_with_superclass!(SliderHostObject);
fn update_touch(env: &mut Environment, slider: id, touch: id, event: id) {
    let point: CGPoint = msg![env; touch locationInView:slider];
    let bounds: CGRect = msg![env; slider bounds];
    let host = env.objc.borrow::<SliderHostObject>(slider);
    let old = host.value;
    let fraction =
        ((point.x - bounds.origin.x - 10.0) / (bounds.size.width - 20.0).max(1.0)).clamp(0.0, 1.0);
    let value = host.minimum + fraction * (host.maximum - host.minimum);
    () = msg![env; slider setValue:value];
    if value != old {
        super::send_actions(env, slider, event, UIControlEventValueChanged);
    }
}
fn set_image(env: &mut Environment, slider: id, kind: u8, state: u32, image: id) {
    retain(env, image);
    let old = env
        .objc
        .borrow_mut::<SliderHostObject>(slider)
        .images
        .insert((kind, state), image);
    if let Some(old) = old {
        release(env, old);
    }
    () = msg![env; slider setNeedsDisplay];
}
fn image_for(env: &Environment, slider: id, kind: u8, state: u32) -> id {
    let host = env.objc.borrow::<SliderHostObject>(slider);
    host.images
        .get(&(kind, state))
        .or_else(|| host.images.get(&(kind, UIControlStateNormal)))
        .copied()
        .unwrap_or(nil)
}
pub const CLASSES: ClassExports = objc_classes! {
(env, this, _cmd);
@implementation UISlider: UIControl
+ (id)allocWithZone:(NSZonePtr)_zone {
    let host = Box::new(SliderHostObject {
        maximum: 1.0,
        ..Default::default()
    });
    env.objc.alloc_object(this, host, &mut env.mem)
}
- (id)initWithFrame:(CGRect)frame {
    msg_super![env; this initWithFrame:frame]
}
- (id)initWithCoder:(id)coder {
    let this: id = msg_super![env; this initWithCoder:coder];
    for (key, slot) in [("UIMinimumValue", 0), ("UIMaximumValue", 1), ("UIValue", 2)] {
        let key = get_static_str(env, key);
        if msg![env; coder containsValueForKey:key] {
            let value: f32 = msg![env; coder decodeFloatForKey:key];
            let host = env.objc.borrow_mut::<SliderHostObject>(this);
            match slot {
                0 => host.minimum = value,
                1 => host.maximum = value,
                _ => host.value = value,
            }
        }
    }
    this
}
- (f32)value {
    env.objc.borrow::<SliderHostObject>(this).value
}
- (())setValue:(f32)value {
    let host = env.objc.borrow_mut::<SliderHostObject>(this);
    host.value = value.max(host.minimum).min(host.maximum);
    () = msg![env; this setNeedsDisplay];
}
- (())setValue:(f32)value animated:(bool)_animated {
    () = msg![env; this setValue:value];
}
- (f32)minimumValue {
    env.objc.borrow::<SliderHostObject>(this).minimum
}
- (())setMinimumValue:(f32)value {
    let host = env.objc.borrow_mut::<SliderHostObject>(this);
    host.minimum = value;
    host.maximum = host.maximum.max(value);
    host.value = host.value.max(value);
    () = msg![env; this setNeedsDisplay];
}
- (f32)maximumValue {
    env.objc.borrow::<SliderHostObject>(this).maximum
}
- (())setMaximumValue:(f32)value {
    let host = env.objc.borrow_mut::<SliderHostObject>(this);
    host.maximum = value;
    host.minimum = host.minimum.min(value);
    host.value = host.value.min(value);
    () = msg![env; this setNeedsDisplay];
}
- (id)tintColor {
    env.objc.borrow::<SliderHostObject>(this).tint
}
- (())setTintColor:(id)color {
    retain(env, color);
    let old = std::mem::replace(
        &mut env.objc.borrow_mut::<SliderHostObject>(this).tint,
        color,
    );
    release(env, old);
    () = msg![env; this setNeedsDisplay];
}
- (())setMinimumValueImage:(id)image {
    retain(env, image);
    let old = std::mem::replace(
        &mut env.objc.borrow_mut::<SliderHostObject>(this).minimum_image,
        image,
    );
    release(env, old);
    () = msg![env; this setNeedsDisplay];
}
- (())setMaximumValueImage:(id)image {
    retain(env, image);
    let old = std::mem::replace(
        &mut env.objc.borrow_mut::<SliderHostObject>(this).maximum_image,
        image,
    );
    release(env, old);
    () = msg![env; this setNeedsDisplay];
}
- (())setMinimumTrackImage:(id)image forState:(u32)state {
    set_image(env, this, 0, state, image);
}
- (())setMaximumTrackImage:(id)image forState:(u32)state {
    set_image(env, this, 1, state, image);
}
- (())setThumbImage:(id)image forState:(u32)state {
    set_image(env, this, 2, state, image);
}
- (bool)beginTrackingWithTouch:(id)touch withEvent:(id)event {
    update_touch(env, this, touch, event);
    true
}
- (bool)continueTrackingWithTouch:(id)touch withEvent:(id)event {
    update_touch(env, this, touch, event);
    true
}
- (())drawRect:(CGRect)_rect {
    let bounds: CGRect = msg![env; this bounds];
    let state: u32 = msg![env; this state];
    let host = env.objc.borrow::<SliderHostObject>(this);
    let fraction = if host.maximum > host.minimum {
        (host.value - host.minimum) / (host.maximum - host.minimum)
    } else {
        0.0
    };
    let tint = host.tint;
    let left = bounds.origin.x + 10.0;
    let width = (bounds.size.width - 20.0).max(0.0);
    let y = bounds.origin.y + bounds.size.height * 0.5;
    let min_rect = CGRect {
        origin: CGPoint {
            x: left,
            y: y - 2.0,
        },
        size: CGSize {
            width: width * fraction,
            height: 4.0,
        },
    };
    let max_rect = CGRect {
        origin: CGPoint {
            x: left + width * fraction,
            y: y - 2.0,
        },
        size: CGSize {
            width: width * (1.0 - fraction),
            height: 4.0,
        },
    };
    let thumb_rect = CGRect {
        origin: CGPoint {
            x: left + width * fraction - 10.0,
            y: y - 10.0,
        },
        size: CGSize {
            width: 20.0,
            height: 20.0,
        },
    };
    for (kind, rect) in [(0, min_rect), (1, max_rect), (2, thumb_rect)] {
        let image = image_for(env, this, kind, state);
        if image != nil {
            () = msg![env; image drawInRect:rect];
        } else {
            let color = if kind == 0 && tint != nil {
                crate::frameworks::uikit::ui_color::get_rgba(&env.objc, tint)
            } else if kind == 0 {
                (0.2, 0.45, 0.9, 1.0)
            } else if kind == 1 {
                (0.6, 0.6, 0.6, 1.0)
            } else {
                (0.9, 0.9, 0.9, 1.0)
            };
            super::control_drawing::fill(env, rect, color);
        }
    }
}
- (())dealloc {
    let host = env.objc.borrow_mut::<SliderHostObject>(this);
    let refs = [host.tint, host.minimum_image, host.maximum_image];
    let images = std::mem::take(&mut host.images);
    for value in refs {
        release(env, value);
    }
    for (_, value) in images {
        release(env, value);
    }
    msg_super![env; this dealloc]
}
@end
};

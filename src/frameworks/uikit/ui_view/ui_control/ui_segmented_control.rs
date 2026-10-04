/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Segmented controls, including archived UISegment subviews.
use super::{UIControlEventValueChanged, UIControlHostObject};
use crate::frameworks::core_graphics::{CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::{ns_string::get_static_str, NSInteger};
use crate::objc::{
    id, msg, msg_class, msg_super, nil, objc_classes, release, retain, ClassExports, NSZonePtr,
};
use crate::{impl_HostObject_with_superclass, Environment};
#[derive(Default)]
struct SegmentedHostObject {
    superclass: UIControlHostObject,
    segments: Vec<id>,
    selected: NSInteger,
    tint: id,
}
impl_HostObject_with_superclass!(SegmentedHostObject);
fn layout(env: &mut Environment, control: id) {
    let bounds: CGRect = msg![env; control bounds];
    let host = env.objc.borrow::<SegmentedHostObject>(control);
    let segments = host.segments.clone();
    let selected = host.selected;
    let width = bounds.size.width / segments.len().max(1) as f32;
    for (i, segment) in segments.into_iter().enumerate() {
        let rect = CGRect {
            origin: CGPoint {
                x: bounds.origin.x + i as f32 * width,
                y: bounds.origin.y,
            },
            size: CGSize {
                width,
                height: bounds.size.height,
            },
        };
        () = msg![env; segment setFrame:rect];
        () = msg![env; segment setUserInteractionEnabled:false];
        () = msg![env; segment setSelected:(i as i32 == selected)];
        () = msg![env; segment setOpaque:false];
        () = msg![env; segment setBackgroundColor:nil];
        () = msg![env; segment layoutSubviews];
        () = msg![env; segment setNeedsDisplay];
    }
    () = msg![env; control setNeedsDisplay];
}
pub const CLASSES: ClassExports = objc_classes! {
(env, this, _cmd);
@implementation UISegmentedControl: UIControl
+ (id)allocWithZone:(NSZonePtr)_zone {
    let host = Box::new(SegmentedHostObject {
        selected: -1,
        ..Default::default()
    });
    env.objc.alloc_object(this, host, &mut env.mem)
}
- (id)initWithFrame:(CGRect)frame {
    msg_super![env; this initWithFrame:frame]
}
- (id)initWithItems:(id)items {
    let this: id = msg_super![env; this initWithFrame:(CGRect::default())];
    let count: u32 = msg![env; items count];
    for i in 0..count {
        let text: id = msg![env; items objectAtIndex:i];
        let segment: id = msg_class![env; UISegment new];
        let label: id = msg_class![env; UILabel new];
        () = msg![env; label setText:text];
        () = msg![env; label setTextAlignment:1i32];
        () = msg![env; segment addSubview:label];
        release(env, label);
        () = msg![env; this addSubview:segment];
        env.objc
            .borrow_mut::<SegmentedHostObject>(this)
            .segments
            .push(segment);
    }
    layout(env, this);
    this
}
- (id)initWithCoder:(id)coder {
    let this: id = msg_super![env; this initWithCoder:coder];
    let key = get_static_str(env, "UISegments");
    let segments: id = msg![env; coder decodeObjectForKey:key];
    let count: u32 = msg![env; segments count];
    let mut values = Vec::new();
    for i in 0..count {
        let segment: id = msg![env; segments objectAtIndex:i];
        retain(env, segment);
        values.push(segment);
        () = msg![env; this addSubview:segment];
    }
    let key = get_static_str(env, "UISelectedSegmentIndex");
    let selected: NSInteger = if msg![env; coder containsValueForKey:key] {
        msg![env; coder decodeIntegerForKey:key]
    } else {
        -1
    };
    let key = get_static_str(env, "UISegmentedControlTintColor");
    let tint: id = msg![env; coder decodeObjectForKey:key];
    retain(env, tint);
    let host = env.objc.borrow_mut::<SegmentedHostObject>(this);
    host.segments = values;
    host.selected = selected;
    host.tint = tint;
    layout(env, this);
    this
}
- (NSInteger)selectedSegmentIndex {
    env.objc.borrow::<SegmentedHostObject>(this).selected
}
- (())setSelectedSegmentIndex:(NSInteger)index {
    env.objc.borrow_mut::<SegmentedHostObject>(this).selected = index;
    layout(env, this);
}
- (NSInteger)numberOfSegments {
    env.objc.borrow::<SegmentedHostObject>(this).segments.len() as i32
}
- (id)tintColor {
    env.objc.borrow::<SegmentedHostObject>(this).tint
}
- (())setTintColor:(id)color {
    retain(env, color);
    let old = std::mem::replace(
        &mut env.objc.borrow_mut::<SegmentedHostObject>(this).tint,
        color,
    );
    release(env, old);
    () = msg![env; this setNeedsDisplay];
}
- (bool)beginTrackingWithTouch:(id)touch withEvent:(id)event {
    let point: CGPoint = msg![env; touch locationInView:this];
    let bounds: CGRect = msg![env; this bounds];
    let count = env.objc.borrow::<SegmentedHostObject>(this).segments.len();
    if count == 0 || bounds.size.width <= 0.0 {
        return false;
    }
    let index = (((point.x - bounds.origin.x) / bounds.size.width * count as f32) as i32)
        .clamp(0, count as i32 - 1);
    let old = env.objc.borrow::<SegmentedHostObject>(this).selected;
    () = msg![env; this setSelectedSegmentIndex:index];
    if old != index {
        super::send_actions(env, this, event, UIControlEventValueChanged);
    }
    true
}
- (())layoutSubviews {
    layout(env, this);
}
- (())drawRect:(CGRect)_rect {
    let bounds: CGRect = msg![env; this bounds];
    let host = env.objc.borrow::<SegmentedHostObject>(this);
    let selected = host.selected;
    let count = host.segments.len();
    let tint = host.tint;
    let width = bounds.size.width / count.max(1) as f32;
    let tint = if tint == nil {
        (0.2, 0.4, 0.8, 1.0)
    } else {
        crate::frameworks::uikit::ui_color::get_rgba(&env.objc, tint)
    };
    for i in 0..count {
        let rect = CGRect {
            origin: CGPoint {
                x: bounds.origin.x + i as f32 * width,
                y: bounds.origin.y,
            },
            size: CGSize {
                width: (width - 1.0).max(0.0),
                height: bounds.size.height,
            },
        };
        super::control_drawing::fill(
            env,
            rect,
            if i as i32 == selected {
                tint
            } else {
                (0.85, 0.85, 0.85, 1.0)
            },
        );
    }
}
- (())dealloc {
    let host = env.objc.borrow_mut::<SegmentedHostObject>(this);
    let tint = host.tint;
    let segments = std::mem::take(&mut host.segments);
    release(env, tint);
    for segment in segments {
        release(env, segment);
    }
    msg_super![env; this dealloc]
}
@end
@implementation UISegment: UIControl
- (id)initWithFrame:(CGRect)frame {
    msg_super![env; this initWithFrame:frame]
}
- (id)initWithCoder:(id)coder {
    let this: id = msg_super![env; this initWithCoder:coder];
    let key = get_static_str(env, "UISegmentInfo");
    let info: id = msg![env; coder decodeObjectForKey:key];
    let string = env.objc.get_known_class("NSString", &mut env.mem);
    if info != nil && msg![env; info isKindOfClass:string] {
        let bounds: CGRect = msg![env; this bounds];
        let label: id = msg_class![env; UILabel alloc];
        let label: id = msg![env; label initWithFrame:bounds];
        () = msg![env; label setText:info];
        () = msg![env; label setTextAlignment:1i32];
        () = msg![env; this addSubview:label];
        release(env, label);
    }
    this
}
- (())layoutSubviews {
    let bounds: CGRect = msg![env; this bounds];
    let subviews: id = msg![env; this subviews];
    let count: u32 = msg![env; subviews count];
    for i in 0..count {
        let view: id = msg![env; subviews objectAtIndex:i];
        () = msg![env; view setFrame:bounds];
    }
}
@end
};

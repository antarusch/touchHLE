/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIProgressView`.

use crate::frameworks::core_graphics::cg_context::{CGContextFillRect, CGContextSetRGBFillColor};
use crate::frameworks::core_graphics::{CGRect, CGSize};
use crate::frameworks::foundation::ns_string::get_static_str;
use crate::frameworks::foundation::NSInteger;
use crate::frameworks::uikit::ui_graphics::UIGraphicsGetCurrentContext;
use crate::impl_HostObject_with_superclass;
use crate::objc::{
    id, msg, msg_super, nil, objc_classes, release, retain, ClassExports, NSZonePtr,
};
use crate::Environment;

type UIProgressViewStyle = NSInteger;

#[derive(Default)]
struct UIProgressViewHostObject {
    superclass: super::UIViewHostObject,
    progress: f32,
    style: UIProgressViewStyle,
    progress_tint_color: id,
    track_tint_color: id,
}
impl_HostObject_with_superclass!(UIProgressViewHostObject);

fn fill(env: &mut Environment, rect: CGRect, color: (f32, f32, f32, f32)) {
    let context = UIGraphicsGetCurrentContext(env);
    CGContextSetRGBFillColor(env, context, color.0, color.1, color.2, color.3);
    CGContextFillRect(env, context, rect);
}

fn color_or_default(
    env: &Environment,
    color: id,
    default: (f32, f32, f32, f32),
) -> (f32, f32, f32, f32) {
    if color == nil {
        default
    } else {
        crate::frameworks::uikit::ui_color::get_rgba(&env.objc, color)
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIProgressView: UIView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UIProgressViewHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithFrame:(CGRect)frame {
    msg_super![env; this initWithFrame:frame]
}

- (id)initWithProgressViewStyle:(UIProgressViewStyle)style {
    let this: id = msg![env; this init];
    env.objc.borrow_mut::<UIProgressViewHostObject>(this).style = style;
    this
}

- (id)initWithCoder:(id)coder {
    let this: id = msg_super![env; this initWithCoder:coder];

    let progress_key = get_static_str(env, "UIProgress");
    if msg![env; coder containsValueForKey:progress_key] {
        let progress: f32 = msg![env; coder decodeFloatForKey:progress_key];
        () = msg![env; this setProgress:progress];
    }

    let style_key = get_static_str(env, "UIProgressViewStyle");
    if msg![env; coder containsValueForKey:style_key] {
        let style: i32 = msg![env; coder decodeIntForKey:style_key];
        () = msg![env; this setProgressViewStyle:style];
    }

    let progress_tint_key = get_static_str(env, "UIProgressProgressTintColor");
    if msg![env; coder containsValueForKey:progress_tint_key] {
        let color: id = msg![env; coder decodeObjectForKey:progress_tint_key];
        () = msg![env; this setProgressTintColor:color];
    }

    let track_tint_key = get_static_str(env, "UIProgressTrackTintColor");
    if msg![env; coder containsValueForKey:track_tint_key] {
        let color: id = msg![env; coder decodeObjectForKey:track_tint_key];
        () = msg![env; this setTrackTintColor:color];
    }

    this
}

- (f32)progress {
    env.objc.borrow::<UIProgressViewHostObject>(this).progress
}

- (())setProgress:(f32)progress {
    env.objc.borrow_mut::<UIProgressViewHostObject>(this).progress = progress.clamp(0.0, 1.0);
    () = msg![env; this setNeedsDisplay];
}

- (())setProgress:(f32)progress animated:(bool)_animated {
    () = msg![env; this setProgress:progress];
}

- (UIProgressViewStyle)progressViewStyle {
    env.objc.borrow::<UIProgressViewHostObject>(this).style
}

- (())setProgressViewStyle:(UIProgressViewStyle)style {
    env.objc.borrow_mut::<UIProgressViewHostObject>(this).style = style;
    () = msg![env; this setNeedsDisplay];
}

- (id)progressTintColor {
    env.objc
        .borrow::<UIProgressViewHostObject>(this)
        .progress_tint_color
}

- (())setProgressTintColor:(id)color {
    retain(env, color);
    let old = std::mem::replace(
        &mut env
            .objc
            .borrow_mut::<UIProgressViewHostObject>(this)
            .progress_tint_color,
        color,
    );
    release(env, old);
    () = msg![env; this setNeedsDisplay];
}

- (id)trackTintColor {
    env.objc
        .borrow::<UIProgressViewHostObject>(this)
        .track_tint_color
}

- (())setTrackTintColor:(id)color {
    retain(env, color);
    let old = std::mem::replace(
        &mut env
            .objc
            .borrow_mut::<UIProgressViewHostObject>(this)
            .track_tint_color,
        color,
    );
    release(env, old);
    () = msg![env; this setNeedsDisplay];
}

- (())drawRect:(CGRect)_rect {
    let bounds: CGRect = msg![env; this bounds];
    let host = env.objc.borrow::<UIProgressViewHostObject>(this);
    let progress = host.progress;
    let progress_color =
        color_or_default(env, host.progress_tint_color, (0.16, 0.47, 0.86, 1.0));
    let track_color =
        color_or_default(env, host.track_tint_color, (0.72, 0.72, 0.72, 1.0));

    fill(env, bounds, track_color);

    let progress_rect = CGRect {
        origin: bounds.origin,
        size: CGSize {
            width: bounds.size.width * progress,
            height: bounds.size.height,
        },
    };
    fill(env, progress_rect, progress_color);
}

- (())dealloc {
    let host = env.objc.borrow_mut::<UIProgressViewHostObject>(this);
    let progress_tint_color = host.progress_tint_color;
    let track_tint_color = host.track_tint_color;
    host.progress_tint_color = nil;
    host.track_tint_color = nil;
    release(env, progress_tint_color);
    release(env, track_tint_color);
    msg_super![env; this dealloc]
}

@end

};

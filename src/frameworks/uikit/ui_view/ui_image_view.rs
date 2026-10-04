/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIImageView`.

use crate::frameworks::core_graphics::cg_image::CGImageRef;
use crate::frameworks::core_graphics::{CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::ns_string::get_static_str;
use crate::frameworks::foundation::{NSInteger, NSTimeInterval, NSUInteger};
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil, objc_classes, release,
    retain, ClassExports, HostObject, NSZonePtr, SEL,
};
use crate::Environment;
use std::time::Instant;

#[derive(Default)]
struct UIImageViewHostObject {
    superclass: super::UIViewHostObject,
    /// `UIImage*`
    image: id,
    /// `UIImage*`
    highlighted_image: id,
    highlighted: bool,
    /// Owned copy of `NSArray<UIImage*>`.
    animation_images: id,
    animation_duration: NSTimeInterval,
    animation_repeat_count: NSInteger,
    /// Owned timer; its target holds a weak reference back to this view.
    animation_timer: id,
    /// Weak: retained by `animation_timer`.
    animation_target: id,
    animation_started: Option<Instant>,
    animation_index: NSUInteger,
}
impl_HostObject_with_superclass!(UIImageViewHostObject);

struct ImageAnimationTarget {
    /// Weak reference, cleared before invalidating or releasing the timer.
    view: id,
}
impl HostObject for ImageAnimationTarget {}

fn animation_cycle(duration: NSTimeInterval, count: NSUInteger) -> NSTimeInterval {
    if duration.is_finite() && duration > 0.0 {
        duration
    } else {
        count as f64 / 30.0
    }
}

fn frame_at_elapsed(
    elapsed: f64,
    cycle: NSTimeInterval,
    count: NSUInteger,
    repeat_count: NSInteger,
) -> Option<NSUInteger> {
    if count == 0 || (repeat_count > 0 && elapsed >= cycle * repeat_count as f64) {
        return None;
    }
    Some((((elapsed % cycle) / cycle * count as f64) as NSUInteger).min(count - 1))
}

fn update_image(env: &mut Environment, this: id) {
    let host_obj = env.objc.borrow::<UIImageViewHostObject>(this);
    let mut image = if host_obj.highlighted && host_obj.highlighted_image != nil {
        host_obj.highlighted_image
    } else {
        // TODO: apply UIKit's default highlight tint when no alternate exists.
        host_obj.image
    };
    let frames = host_obj.animation_images;
    let index = host_obj.animation_index;
    let animated = host_obj.animation_started.is_some() && !host_obj.highlighted;
    if animated {
        let count: NSUInteger = msg![env; frames count];
        if count > 0 {
            image = msg![env; frames objectAtIndex:(index.min(count - 1))];
        }
    }
    let layer: id = msg![env; this layer];
    let cg_image: CGImageRef = msg![env; image CGImage];
    () = msg![env; layer setContents:cg_image];
}

fn stop_animation(env: &mut Environment, this: id) {
    let host_obj = env.objc.borrow_mut::<UIImageViewHostObject>(this);
    let timer = std::mem::replace(&mut host_obj.animation_timer, nil);
    let target = std::mem::replace(&mut host_obj.animation_target, nil);
    host_obj.animation_started = None;
    host_obj.animation_index = 0;
    if target != nil {
        env.objc.borrow_mut::<ImageAnimationTarget>(target).view = nil;
    }
    if timer != nil {
        () = msg![env; timer invalidate];
        release(env, timer);
    }
}

fn start_animation(env: &mut Environment, this: id) {
    stop_animation(env, this);
    let host_obj = env.objc.borrow::<UIImageViewHostObject>(this);
    let images = host_obj.animation_images;
    let duration = host_obj.animation_duration;
    let count: NSUInteger = msg![env; images count];
    if count == 0 {
        update_image(env, this);
        return;
    }
    let cycle = animation_cycle(duration, count);
    let class = env
        .objc
        .get_known_class("_touchHLEImageAnimationTarget", &mut env.mem);
    let target = env.objc.alloc_object(
        class,
        Box::new(ImageAnimationTarget { view: this }),
        &mut env.mem,
    );
    let selector: SEL = env
        .objc
        .lookup_selector("_touchHLE_imageAnimationTimerDidFire:")
        .unwrap();
    let started = Instant::now();
    let timer: id = msg_class![env; NSTimer scheduledTimerWithTimeInterval:(cycle / count as f64)
                          target:target selector:selector userInfo:nil repeats:true];
    retain(env, timer);
    release(env, target);
    let host_obj = env.objc.borrow_mut::<UIImageViewHostObject>(this);
    host_obj.animation_timer = timer;
    host_obj.animation_target = target;
    host_obj.animation_started = Some(started);
    update_image(env, this);
}

fn restart_if_animating(env: &mut Environment, this: id) {
    if env
        .objc
        .borrow::<UIImageViewHostObject>(this)
        .animation_started
        .is_some()
    {
        start_animation(env, this);
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIImageView: UIView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UIImageViewHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithFrame:(CGRect)frame {
    let this: id = msg_super![env; this initWithFrame:frame];
    // Not sure if UIImageView does this unconditionally, or only for images
    // with alpha channels.
    () = msg![env; this setOpaque:false];
    () = msg![env; this setUserInteractionEnabled:false];
    this
}

- (())dealloc {
    stop_animation(env, this);
    let &UIImageViewHostObject {
        superclass: _,
        image,
        highlighted_image,
        animation_images,
        ..
    } = env.objc.borrow(this);
    release(env, image);
    release(env, highlighted_image);
    release(env, animation_images);
    msg_super![env; this dealloc]
}

// NSCoding implementation
- (id)initWithCoder:(id)coder {
    let this: id = msg_super![env; this initWithCoder:coder];

    let key_ns_string = get_static_str(env, "UIImage");
    let image: id = msg![env; coder decodeObjectForKey:key_ns_string];

    () = msg![env; this setImage:image];
    let key_ns_string = get_static_str(env, "UIHighlightedImage");
    let highlighted_image: id = msg![env; coder decodeObjectForKey:key_ns_string];
    () = msg![env; this setHighlightedImage:highlighted_image];
    let key_ns_string = get_static_str(env, "UIHighlighted");
    let highlighted: bool = msg![env; coder decodeBoolForKey:key_ns_string];
    () = msg![env; this setHighlighted:highlighted];
    () = msg![env; this setOpaque:false];
    let key_ns_string = get_static_str(env, "UIUserInteractionDisabled");
    let has_interaction: bool = msg![env; coder containsValueForKey:key_ns_string];
    let enabled = if has_interaction {
        let disabled: bool = msg![env; coder decodeBoolForKey:key_ns_string];
        !disabled
    } else {
        false
    };
    () = msg![env; this setUserInteractionEnabled:enabled];

    this
}

- (id)initWithImage:(id)image { // UIImage*
    msg![env; this initWithImage:image highlightedImage:nil]
}

- (id)initWithImage:(id)image // UIImage*
  highlightedImage:(id)highlighted_image { // UIImage*
    let sizing_image = if image != nil { image } else { highlighted_image };
    let size: CGSize = msg![env; sizing_image size];
    let frame = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size,
    };
    let this: id = msg_super![env; this initWithFrame:frame];
    () = msg![env; this setImage:image];
    () = msg![env; this setHighlightedImage:highlighted_image];
    () = msg![env; this setOpaque:false];
    () = msg![env; this setUserInteractionEnabled:false];
    this
}

- (id)image {
    env.objc.borrow::<UIImageViewHostObject>(this).image
}

- (())setImage:(id)new_image { // UIImage*
    // Retain first so assigning the same image remains valid.
    retain(env, new_image);
    let old_image = std::mem::replace(
        &mut env.objc.borrow_mut::<UIImageViewHostObject>(this).image,
        new_image,
    );
    release(env, old_image);
    update_image(env, this);
}

- (id)highlightedImage {
    env.objc.borrow::<UIImageViewHostObject>(this).highlighted_image
}
- (())setHighlightedImage:(id)new_image { // UIImage*
    retain(env, new_image);
    let old_image = std::mem::replace(
        &mut env.objc.borrow_mut::<UIImageViewHostObject>(this).highlighted_image,
        new_image,
    );
    release(env, old_image);
    update_image(env, this);
}

- (bool)isHighlighted {
    env.objc.borrow::<UIImageViewHostObject>(this).highlighted
}
- (())setHighlighted:(bool)highlighted {
    env.objc.borrow_mut::<UIImageViewHostObject>(this).highlighted = highlighted;
    update_image(env, this);
}

- (id)animationImages {
    env.objc.borrow::<UIImageViewHostObject>(this).animation_images
}
- (())setAnimationImages:(id)images { // NSArray<UIImage*>*
    let images: id = msg![env; images copy];
    let old = std::mem::replace(
        &mut env.objc.borrow_mut::<UIImageViewHostObject>(this).animation_images,
        images,
    );
    release(env, old);
    restart_if_animating(env, this);
}

- (NSTimeInterval)animationDuration {
    env.objc.borrow::<UIImageViewHostObject>(this).animation_duration
}
- (())setAnimationDuration:(NSTimeInterval)duration {
    assert!(duration.is_finite() && duration >= 0.0);
    env.objc.borrow_mut::<UIImageViewHostObject>(this).animation_duration = duration;
    restart_if_animating(env, this);
}

- (NSInteger)animationRepeatCount {
    env.objc.borrow::<UIImageViewHostObject>(this).animation_repeat_count
}
- (())setAnimationRepeatCount:(NSInteger)count {
    assert!(count >= 0);
    env.objc.borrow_mut::<UIImageViewHostObject>(this).animation_repeat_count = count;
    restart_if_animating(env, this);
}

- (bool)isAnimating {
    env.objc.borrow::<UIImageViewHostObject>(this).animation_started.is_some()
}
- (())startAnimating {
    start_animation(env, this);
}
- (())stopAnimating {
    stop_animation(env, this);
    update_image(env, this);
}

@end

@implementation _touchHLEImageAnimationTarget: NSObject

- (())_touchHLE_imageAnimationTimerDidFire:(id)_timer {
    let view = env.objc.borrow::<ImageAnimationTarget>(this).view;
    if view == nil {
        return;
    }
    let host_obj = env.objc.borrow::<UIImageViewHostObject>(view);
    let Some(started) = host_obj.animation_started else { return; };
    let frames = host_obj.animation_images;
    let duration = host_obj.animation_duration;
    let repeats = host_obj.animation_repeat_count;
    let count: NSUInteger = msg![env; frames count];
    let cycle = animation_cycle(duration, count);
    if let Some(index) = frame_at_elapsed(started.elapsed().as_secs_f64(), cycle, count, repeats) {
        env.objc.borrow_mut::<UIImageViewHostObject>(view).animation_index = index;
    } else {
        stop_animation(env, view);
    }
    update_image(env, view);
}

@end

};

#[cfg(test)]
mod tests {
    use super::{animation_cycle, frame_at_elapsed};

    #[test]
    fn image_animation_frames_and_repeats() {
        assert_eq!(animation_cycle(0.0, 30), 1.0);
        assert_eq!(animation_cycle(2.0, 3), 2.0);
        assert_eq!(frame_at_elapsed(0.0, 1.0, 4, 1), Some(0));
        assert_eq!(frame_at_elapsed(0.25, 1.0, 4, 1), Some(1));
        assert_eq!(frame_at_elapsed(0.75, 1.0, 4, 1), Some(3));
        assert_eq!(frame_at_elapsed(1.0, 1.0, 4, 1), None);
        assert_eq!(frame_at_elapsed(1.25, 1.0, 4, 2), Some(1));
        assert_eq!(frame_at_elapsed(2.0, 1.0, 4, 2), None);
        assert_eq!(frame_at_elapsed(1000.25, 1.0, 4, 0), Some(1));
        assert_eq!(frame_at_elapsed(0.0, 1.0, 0, 0), None);
    }
}

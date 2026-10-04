/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIButton`.

use super::{UIControlState, UIControlStateNormal};
use crate::frameworks::core_graphics::{CGFloat, CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::ns_string::{from_rust_string, get_static_str};
use crate::frameworks::foundation::NSInteger;
use crate::frameworks::uikit::ui_font::UITextAlignmentCenter;
use crate::objc::{
    autorelease, id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil, objc_classes,
    release, retain, todo_objc_setter, ClassExports, HostObject, NSZonePtr,
};
use crate::Environment;
use std::collections::HashMap;

type UIButtonType = NSInteger;
pub const UIButtonTypeCustom: UIButtonType = 0;
pub const UIButtonTypeRoundedRect: UIButtonType = 1;
#[allow(dead_code)]
const UIButtonTypeDetailDisclosure: UIButtonType = 2;
#[allow(dead_code)]
const UIButtonTypeInfoLight: UIButtonType = 3;
#[allow(dead_code)]
const UIButtonTypeInfoDark: UIButtonType = 4;
#[allow(dead_code)]
const UIButtonTypeContactAdd: UIButtonType = 5;

// Host object for an intermediate object
// used for decoding of UIButton from a NIB
#[derive(Default)]
struct UIButtonContentHostObject {
    /// `NSString*`
    title: id,
    /// `UIColor*`
    title_color: id,
    /// `UIImage*`
    image: id,
    /// `UIImage*`
    background_image: id,
}
impl HostObject for UIButtonContentHostObject {}

pub struct UIButtonHostObject {
    superclass: super::UIControlHostObject,
    type_: UIButtonType,
    /// `UILabel*`
    title_label: id,
    /// `UIImageView*`
    image_view: id,
    /// `UIImageView*`
    background_image_view: id,
    /// Values are `UIString*`
    titles_for_states: HashMap<UIControlState, id>,
    /// Values are `UIColor*`
    title_colors_for_states: HashMap<UIControlState, id>,
    /// Values are `UIImage*`
    images_for_states: HashMap<UIControlState, id>,
    /// Values are `UIImage*`
    background_images_for_states: HashMap<UIControlState, id>,
}
impl_HostObject_with_superclass!(UIButtonHostObject);
impl Default for UIButtonHostObject {
    fn default() -> Self {
        UIButtonHostObject {
            superclass: Default::default(),
            type_: UIButtonTypeCustom,
            title_label: nil,
            image_view: nil,
            background_image_view: nil,
            titles_for_states: HashMap::new(),
            title_colors_for_states: HashMap::new(),
            images_for_states: HashMap::new(),
            background_images_for_states: HashMap::new(),
        }
    }
}

fn update(env: &mut Environment, this: id) {
    let title_label: id = msg![env; this titleLabel];
    let title: id = msg![env; this currentTitle];
    () = msg![env; title_label setText:title];
    let title_color: id = msg![env; this currentTitleColor];
    () = msg![env; title_label setTextColor:title_color];

    let image_view: id = msg![env; this imageView];
    let image: id = msg![env; this currentImage];
    () = msg![env; image_view setImage:image];

    let background_image_view: id = msg![env; this backgroundImageView];
    let background_image: id = msg![env; this currentBackgroundImage];
    () = msg![env; background_image_view setImage:background_image];
    layout(env, this);
}

fn layout(env: &mut Environment, this: id) {
    let label = env.objc.borrow_mut::<UIButtonHostObject>(this).title_label;
    let background_image_view = env
        .objc
        .borrow_mut::<UIButtonHostObject>(this)
        .background_image_view;
    let bounds: CGRect = msg![env; this bounds];

    () = msg![env; background_image_view setFrame:bounds];
    () = msg![env; label setFrame:bounds];
    let image_view = env.objc.borrow::<UIButtonHostObject>(this).image_view;
    let image: id = msg![env; this currentImage];
    let image_size: CGSize = msg![env; image size];
    // Center image-only content at its intrinsic size, constrained to the
    // button's bounds. The default UIImageView starts with a zero-sized frame.
    let size = CGSize {
        width: image_size.width.min(bounds.size.width),
        height: image_size.height.min(bounds.size.height),
    };
    let image_frame = CGRect {
        origin: CGPoint {
            x: bounds.origin.x + (bounds.size.width - size.width) / 2.0,
            y: bounds.origin.y + (bounds.size.height - size.height) / 2.0,
        },
        size,
    };
    () = msg![env; image_view setFrame:image_frame];
    // TODO: jointly lay out images and text, and support content edge insets.
}

fn init_common(env: &mut Environment, this: id) -> id {
    () = msg![env; this setOpaque:false];
    let bg_color: id = msg_class![env; UIColor clearColor];

    let title_label: id = msg_class![env; UILabel new];
    () = msg![env; title_label setBackgroundColor:bg_color];
    () = msg![env; title_label setTextAlignment:UITextAlignmentCenter];

    let text_color: id = msg_class![env; UIColor whiteColor];

    let image_view: id = msg_class![env; UIImageView new];
    let background_image_view: id = msg_class![env; UIImageView new];

    let host_obj = env.objc.borrow_mut::<UIButtonHostObject>(this);
    host_obj.title_label = title_label;
    host_obj.image_view = image_view;
    host_obj.background_image_view = background_image_view;
    host_obj.titles_for_states.insert(UIControlStateNormal, nil);
    host_obj
        .title_colors_for_states
        .insert(UIControlStateNormal, text_color);
    host_obj.images_for_states.insert(UIControlStateNormal, nil);
    host_obj
        .background_images_for_states
        .insert(UIControlStateNormal, nil);

    () = msg![env; this addSubview:background_image_view];
    () = msg![env; this addSubview:title_label];
    () = msg![env; this addSubview:image_view];
    update(env, this);

    this
}
// TODO: refactor this to be a part of common init
fn set_type(env: &mut Environment, button: id, type_: UIButtonType) {
    match type_ {
        UIButtonTypeCustom => (),
        UIButtonTypeRoundedRect => {
            let bg_color: id = msg_class![env; UIColor whiteColor];
            // TODO: set blue background image in highlighted state
            // TODO: image highlighting?
            () = msg![env; button setBackgroundColor:bg_color];
            // On the real iPhone OS, this is a semi-dark, desaturated blue.
            // Should we match it?
            let text_color: id = msg_class![env; UIColor blackColor];
            () = msg![env; button setTitleColor:text_color
                                       forState:UIControlStateNormal];
            let layer: id = msg![env; button layer];
            () = msg![env; layer setCornerRadius:(10.0 as CGFloat)];
            // TODO: set border, once supported
        }
        _ => {
            log!("TODO: UIButtonType {}", type_);
        }
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIButton: UIControl

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UIButtonHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

+ (id)buttonWithType:(UIButtonType)type_ {
    let button: id = msg![env; this new];
    set_type(env, button, type_);
    autorelease(env, button)
}

- (id)initWithFrame:(CGRect)frame {
    let this: id = msg_super![env; this initWithFrame:frame];

    let bg_color: id = msg_class![env; UIColor clearColor];
    () = msg![env; this setBackgroundColor:bg_color];

    let this: id = init_common(env, this);
    // TODO: check which type is a default one
    set_type(env, this, UIButtonTypeCustom);
    this
}

- (id)initWithCoder:(id)coder {
    let this: id = msg_super![env; this initWithCoder:coder];

    let this = init_common(env, this);

    let key_ns_string = get_static_str(env, "UIButtonType");
    let type_: i32 = msg![env; coder decodeIntForKey:key_ns_string];
    set_type(env, this, type_);

    let key_ns_string = get_static_str(env, "UIButtonStatefulContent");
    let dict: id = msg![env; coder decodeObjectForKey:key_ns_string];
    // NIB dictionaries use NSNumber control-state masks as keys. Decode each
    // state: image-only buttons have no title in their normal state.
    let keys: id = msg![env; dict allKeys];
    let count: u32 = msg![env; keys count];
    for i in 0..count {
        let key: id = msg![env; keys objectAtIndex:i];
        let state: UIControlState = msg![env; key unsignedIntValue];
        let content: id = msg![env; dict objectForKey:key];
        let title: id = msg![env; content title];
        if title != nil {
            () = msg![env; this setTitle:title forState:state];
        }
        let color: id = msg![env; content titleColor];
        if color != nil {
            () = msg![env; this setTitleColor:color forState:state];
        }
        let image: id = msg![env; content image];
        if image != nil {
            () = msg![env; this setImage:image forState:state];
        }
        let background: id = msg![env; content backgroundImage];
        if background != nil {
            () = msg![env; this setBackgroundImage:background forState:state];
        }
    }
    update(env, this);

    this
}

- (())dealloc {
    let UIButtonHostObject {
        superclass: _,
        type_: _,
        title_label,
        image_view,
        background_image_view,
        titles_for_states,
        title_colors_for_states,
        images_for_states,
        background_images_for_states
    } = std::mem::take(env.objc.borrow_mut(this));

    release(env, title_label);
    release(env, image_view);
    release(env, background_image_view);
    for (_state, title) in titles_for_states {
        release(env, title);
    }
    for (_state, color) in title_colors_for_states {
        release(env, color);
    }
    for (_state, image) in images_for_states {
        release(env, image);
    }
    for (_state, background_image) in background_images_for_states {
        release(env, background_image);
    }
    msg_super![env; this dealloc]
}

- (())layoutSubviews {
    layout(env, this);
}

- (CGSize)sizeThatFits:(CGSize)size {
    let image: id = msg![env; this currentImage];
    let background: id = msg![env; this currentBackgroundImage];
    let title: id = msg![env; this currentTitle];
    let image_size: CGSize = msg![env; image size];
    let background_size: CGSize = msg![env; background size];
    let title_size = if title != nil {
        let label: id = msg![env; this titleLabel];
        let font: id = msg![env; label font];
        msg![env; title sizeWithFont:font]
    } else {
        CGSize { width: 0.0, height: 0.0 }
    };
    // Image buttons are commonly created with CGRectZero, then sized before
    // placement. Their content size must not be constrained by those bounds.
    if image == nil && background == nil && title == nil {
        return msg_super![env; this sizeThatFits:size];
    }
    CGSize {
        width: (image_size.width + title_size.width).max(background_size.width).ceil(),
        height: image_size.height.max(title_size.height).max(background_size.height).ceil(),
    }
}

- (UIButtonType)buttonType {
    env.objc.borrow_mut::<UIButtonHostObject>(this).type_
}

- (id)titleLabel {
    env.objc.borrow_mut::<UIButtonHostObject>(this).title_label
}

- (id)imageView {
    env.objc.borrow_mut::<UIButtonHostObject>(this).image_view
}

- (id)backgroundImageView {
    env.objc.borrow_mut::<UIButtonHostObject>(this).background_image_view
}
- (())setEnabled:(bool)enabled {
    () = msg_super![env; this setEnabled:enabled];
    update(env, this);
}
- (())setSelected:(bool)selected {
    () = msg_super![env; this setSelected:selected];
    update(env, this);
}
- (())setHighlighted:(bool)highlighted {
    () = msg_super![env; this setHighlighted:highlighted];
    update(env, this);
}
- (())setAdjustsImageWhenHighlighted:(bool)adjusts {
    todo_objc_setter!(this, adjusts);
}
- (())setAdjustsImageWhenDisabled:(bool)adjusts {
    todo_objc_setter!(this, adjusts);
}
- (())setShowsTouchWhenHighlighted:(bool)shows {
    todo_objc_setter!(this, shows);
}
- (())setFont:(id)font { // UIFont*
    let label = env.objc.borrow_mut::<UIButtonHostObject>(this).title_label;
    () = msg![env; label setFont:font];
    update(env, this);
}
// TODO: observe focussing somehow

- (id)currentTitle {
    let state: UIControlState = msg![env; this state];
    msg![env; this titleForState:state]
}
- (id)titleForState:(UIControlState)state {
    let host_obj = env.objc.borrow::<UIButtonHostObject>(this);
    host_obj.titles_for_states.get(&state).or_else(|| {
        host_obj.titles_for_states.get(&UIControlStateNormal)
    }).copied().unwrap()
}
- (())setTitle:(id)title // NSString*
      forState:(UIControlState)state {
    retain(env, title);
    let host_obj = env.objc.borrow_mut::<UIButtonHostObject>(this);
    if let Some(old) = host_obj.titles_for_states.insert(state, title) {
        release(env, old);
    }
    update(env, this);
}

- (id)currentBackgroundImage {
    let state: UIControlState = msg![env; this state];
    msg![env; this backgroundImageForState:state]
}
- (id)backgroundImageForState:(UIControlState)state {
    let host_obj = env.objc.borrow::<UIButtonHostObject>(this);
    host_obj.background_images_for_states.get(&state).or_else(|| {
        host_obj.background_images_for_states.get(&UIControlStateNormal)
    }).copied().unwrap()
}
- (())setBackgroundImage:(id)image forState:(UIControlState)state {
    retain(env,image);
    let host_obj = env.objc.borrow_mut::<UIButtonHostObject>(this);
    // A nil alternate image clears that state's override, allowing UIKit's
    // normal-state fallback when artwork is absent.
    let old = if image == nil && state != UIControlStateNormal {
        host_obj.background_images_for_states.remove(&state)
    } else {
        host_obj.background_images_for_states.insert(state, image)
    };
    if let Some(old) = old {
        release(env, old);
    }
    update(env, this);
}

- (id)currentTitleColor {
    let state: UIControlState = msg![env; this state];
    msg![env; this titleColorForState:state]
}
- (id)titleColorForState:(UIControlState)state {
    let host_obj = env.objc.borrow::<UIButtonHostObject>(this);
    host_obj.title_colors_for_states.get(&state).or_else(|| {
        host_obj.title_colors_for_states.get(&UIControlStateNormal)
    }).copied().unwrap()
}
- (())setTitleColor:(id)color // UIColor*
      forState:(UIControlState)state {
    retain(env, color);
    let host_obj = env.objc.borrow_mut::<UIButtonHostObject>(this);
    if let Some(old) = host_obj.title_colors_for_states.insert(state, color) {
        release(env, old);
    }
    update(env, this);
}

- (id)currentImage {
    let state: UIControlState = msg![env; this state];
    msg![env; this imageForState:state]
}
- (id)imageForState:(UIControlState)state {
    let host_obj = env.objc.borrow::<UIButtonHostObject>(this);
    host_obj.images_for_states.get(&state).or_else(|| {
        host_obj.images_for_states.get(&UIControlStateNormal)
    }).copied().unwrap()
}
- (())setImage:(id)image // UIImage*
      forState:(UIControlState)state {
    retain(env, image);
    let host_obj = env.objc.borrow_mut::<UIButtonHostObject>(this);
    // A nil alternate image clears that state's override, allowing UIKit's
    // normal-state fallback when artwork is absent.
    let old = if image == nil && state != UIControlStateNormal {
        host_obj.images_for_states.remove(&state)
    } else {
        host_obj.images_for_states.insert(state, image)
    };
    if let Some(old) = old {
        release(env, old);
    }
    update(env, this);
}

// TODO: actions, etc

- (id)hitTest:(CGPoint)point
    withEvent:(id)event { // UIEvent* (possibly nil)
    // Hide subviews from hit testing so event goes straight to this control
    if msg![env; this pointInside:point withEvent:event] {
        this
    } else {
        nil
    }
}

@end

// Undocumented classes used by NIBs

@implementation UIRoundedRectButton: UIButton
// TODO: rendering of round corners
@end

@implementation UIButtonContent: NSObject

+ (id)alloc {
    let host_object = Box::<UIButtonContentHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

// NSCoding implementation
- (id)initWithCoder:(id)coder {
    let title_key = get_static_str(env, "UITitle");
    let title: id = msg![env; coder decodeObjectForKey:title_key];
    log_dbg!("UIButtonContent: UITitle -> {:?}", title);

    let title_color_key = get_static_str(env, "UITitleColor");
    let title_color: id = msg![env; coder decodeObjectForKey:title_color_key];
    log_dbg!("UIButtonContent: UITitleColor -> {:?}", title_color);

    let image_key = get_static_str(env, "UIImage");
    let image: id = msg![env; coder decodeObjectForKey:image_key];
    let background_key = get_static_str(env, "UIBackgroundImage");
    let background_image: id = msg![env; coder decodeObjectForKey:background_key];

    retain(env, image);
    retain(env, background_image);
    retain(env, title);
    retain(env, title_color);
    let host_obj = env.objc.borrow_mut::<UIButtonContentHostObject>(this);
    host_obj.title = title;
    host_obj.title_color = title_color;
    host_obj.image = image;
    host_obj.background_image = background_image;

    this
}

- (id)title {
    env.objc.borrow::<UIButtonContentHostObject>(this).title
}
- (id)titleColor {
    env.objc.borrow::<UIButtonContentHostObject>(this).title_color
}

- (id)image {
    env.objc.borrow::<UIButtonContentHostObject>(this).image
}
- (id)backgroundImage {
    env.objc.borrow::<UIButtonContentHostObject>(this).background_image
}

- (id)description {
    let title = env.objc.borrow::<UIButtonContentHostObject>(this).title;
    let title_color = env.objc.borrow::<UIButtonContentHostObject>(this).title_color;
    let desc_str = format!(
        "UIButtonContent({this:?}, title {title:?}, title_color {title_color:?})"
    );
    let desc = from_rust_string(env, desc_str);
    autorelease(env, desc)
}

- (())dealloc {
    let &UIButtonContentHostObject {
        title,
        title_color,
        image,
        background_image,
    } = env.objc.borrow(this);
    release(env, title);
    release(env, title_color);
    release(env, image);
    release(env, background_image);

    env.objc.dealloc_object(this, &mut env.mem)
}

@end

};

/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIBarItem` and `UIBarButtonItem`.

use crate::frameworks::foundation::NSInteger;
use crate::objc::{
    id, nil, objc_classes, release, retain, ClassExports, HostObject, NSZonePtr, SEL,
};

type UIBarButtonItemStyle = NSInteger;
type UIBarButtonSystemItem = NSInteger;

struct UIBarButtonItemHostObject {
    title: id,
    custom_view: id,
    target: id,
    action: Option<SEL>,
    style: UIBarButtonItemStyle,
    system_item: Option<UIBarButtonSystemItem>,
    enabled: bool,
}
impl HostObject for UIBarButtonItemHostObject {}

impl Default for UIBarButtonItemHostObject {
    fn default() -> Self {
        Self {
            title: nil,
            custom_view: nil,
            target: nil,
            action: None,
            style: 0,
            system_item: None,
            enabled: true,
        }
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIBarItem: NSObject
@end

@implementation UIBarButtonItem: UIBarItem

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UIBarButtonItemHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (())dealloc {
    let host = env.objc.borrow::<UIBarButtonItemHostObject>(this);
    let title = host.title;
    let custom_view = host.custom_view;
    release(env, title);
    release(env, custom_view);
    env.objc.dealloc_object(this, &mut env.mem);
}

- (id)initWithBarButtonSystemItem:(UIBarButtonSystemItem)system_item
                           target:(id)target
                           action:(SEL)action {
    let host = env.objc.borrow_mut::<UIBarButtonItemHostObject>(this);
    host.system_item = Some(system_item);
    host.target = target; // UIKit target is non-retaining.
    host.action = Some(action);
    this
}

- (id)initWithTitle:(id)title
              style:(UIBarButtonItemStyle)style
             target:(id)target
             action:(SEL)action {
    retain(env, title);
    let host = env.objc.borrow_mut::<UIBarButtonItemHostObject>(this);
    release(env, host.title);
    host.title = title;
    host.style = style;
    host.target = target; // UIKit target is non-retaining.
    host.action = Some(action);
    this
}

- (id)initWithCustomView:(id)view {
    retain(env, view);
    let host = env.objc.borrow_mut::<UIBarButtonItemHostObject>(this);
    release(env, host.custom_view);
    host.custom_view = view;
    this
}

- (id)title {
    env.objc.borrow::<UIBarButtonItemHostObject>(this).title
}

- (())setTitle:(id)title {
    retain(env, title);
    let old = env.objc.borrow::<UIBarButtonItemHostObject>(this).title;
    env.objc.borrow_mut::<UIBarButtonItemHostObject>(this).title = title;
    release(env, old);
}

- (bool)isEnabled {
    env.objc.borrow::<UIBarButtonItemHostObject>(this).enabled
}

- (bool)enabled {
    env.objc.borrow::<UIBarButtonItemHostObject>(this).enabled
}

- (())setEnabled:(bool)enabled {
    env.objc.borrow_mut::<UIBarButtonItemHostObject>(this).enabled = enabled;
}

- (id)target {
    env.objc.borrow::<UIBarButtonItemHostObject>(this).target
}

- (())setTarget:(id)target {
    env.objc.borrow_mut::<UIBarButtonItemHostObject>(this).target = target;
}

- (SEL)action {
    env.objc
        .borrow::<UIBarButtonItemHostObject>(this)
        .action
        .expect("UIBarButtonItem action requested before being set")
}

- (())setAction:(SEL)action {
    env.objc.borrow_mut::<UIBarButtonItemHostObject>(this).action = Some(action);
}

- (id)customView {
    env.objc.borrow::<UIBarButtonItemHostObject>(this).custom_view
}

@end

};

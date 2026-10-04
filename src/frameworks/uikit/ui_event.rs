/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIEvent`.

use crate::frameworks::foundation::{NSInteger, NSTimeInterval, NSUInteger};
use crate::mem::MutVoidPtr;
use crate::objc::{
    autorelease, id, msg, msg_class, nil, objc_classes, release, retain, ClassExports, HostObject,
    NSZonePtr,
};
use crate::Environment;

pub type UIEventType = NSInteger;
pub const UIEventTypeTouches: UIEventType = 0;

pub(super) struct UIEventHostObject {
    /// `NSSet<UITouch*>*`
    touches: id,
    timestamp: NSTimeInterval,
}
impl HostObject for UIEventHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIEvent: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(UIEventHostObject {
        touches: nil,
        timestamp: 0.0,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (())dealloc {
    let &UIEventHostObject { touches, .. } = env.objc.borrow(this);
    release(env, touches);
    env.objc.dealloc_object(this, &mut env.mem)
}

- (UIEventType)type {
    UIEventTypeTouches
}

- (NSTimeInterval)timestamp {
    env.objc.borrow::<UIEventHostObject>(this).timestamp
}

- (id)touchesForView:(id)view_ {
    let touches: id = msg![env; this allTouches];

    let touches_for_view: id = msg_class![env; NSMutableSet allocWithZone:(MutVoidPtr::null())];

    let touches_arr: id = msg![env; touches allObjects];
    let touches_count: NSUInteger = msg![env; touches_arr count];
    for i in 0..touches_count {
        let touch: id = msg![env; touches_arr objectAtIndex:i];
        let view: id = msg![env; touch view];
        if view_ == view {
            let _: () = msg![env; touches_for_view addObject:touch];
            if !msg![env; view isMultipleTouchEnabled] {
                break;
            }
        }
    }

    autorelease(env, touches_for_view)
}

- (id)touchesForWindow:(id)window {
    let touches: id = msg![env; this allTouches];
    let result: id = msg_class![env; NSMutableSet new];
    let array: id = msg![env; touches allObjects];
    let count: NSUInteger = msg![env; array count];
    for i in 0..count {
        let touch: id = msg![env; array objectAtIndex:i];
        let touch_window: id = msg![env; touch window];
        if touch_window == window {
            () = msg![env; result addObject:touch];
        }
    }
    autorelease(env, result)
}

- (id)allTouches {
    let &UIEventHostObject { touches, .. } = env.objc.borrow(this);
    touches
}

// TODO: more accessors

@end

};

/// For use by [super::ui_touch]: create a `UIEvent` with a set of `UITouch*`
pub(super) fn new_event(env: &mut Environment, touches: id) -> id {
    let event: id = msg_class![env; UIEvent alloc];
    retain(env, touches);
    let timestamp: NSTimeInterval = {
        let process_info = msg_class![env; NSProcessInfo processInfo];
        msg![env; process_info systemUptime]
    };
    let borrow = env.objc.borrow_mut::<UIEventHostObject>(event);
    borrow.touches = touches;
    borrow.timestamp = timestamp;
    event
}

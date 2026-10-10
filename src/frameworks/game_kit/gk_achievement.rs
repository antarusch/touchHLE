/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Game Center achievements. Online Game Center storage is not emulated.

use crate::abi::{CallFromHost, GuestFunction};
use crate::frameworks::foundation::ns_array;
use crate::mem::MutPtr;
use crate::objc::{id, nil, objc_classes, release, retain, ClassExports, HostObject, NSZonePtr};

struct GKAchievementHostObject {
    identifier: id,
    percent_complete: f64,
}

impl HostObject for GKAchievementHostObject {}

/// An offline Game Center report still needs to run its completion handler:
/// otherwise games can get stuck when advancing past the results screen.
fn finish_report(env: &mut crate::Environment, completion: MutPtr<u8>) {
    if completion.is_null() {
        return;
    }
    let callback_address: u32 = env.mem.read((completion + 12).cast());
    if callback_address != 0 {
        let callback = GuestFunction::from_addr_with_thumb_bit(callback_address);
        let _: () = callback.call_from_host(env, (completion, nil));
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation GKAchievement: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    env.objc.alloc_object(
        this,
        Box::new(GKAchievementHostObject {
            identifier: nil,
            percent_complete: 0.0,
        }),
        &mut env.mem,
    )
}

+ (())loadAchievementsWithCompletionHandler:(MutPtr<u8>)completion {
    // No achievements are available without a real Game Center connection.
    // Always invoke the completion so post-mission progression isn't stalled.
    if !completion.is_null() {
        let callback_address: u32 = env.mem.read((completion + 12).cast());
        if callback_address != 0 {
            let callback = GuestFunction::from_addr_with_thumb_bit(callback_address);
            let achievements = ns_array::from_vec(env, Vec::<id>::new());
            let _: () = callback.call_from_host(env, (completion, achievements, nil));
        }
    }
}

- (id)initWithIdentifier:(id)identifier {
    if identifier == nil {
        return nil;
    }
    retain(env, identifier);
    let old_identifier = env.objc.borrow_mut(this).identifier;
    env.objc.borrow_mut(this).identifier = identifier;
    if old_identifier != nil {
        release(env, old_identifier);
    }
    this
}

- (())dealloc {
    let identifier = env.objc.borrow(this).identifier;
    if identifier != nil {
        release(env, identifier);
    }
    env.objc.dealloc_object(this, &mut env.mem);
}

- (id)identifier {
    env.objc.borrow(this).identifier
}

- (f64)percentComplete {
    env.objc.borrow(this).percent_complete
}

- (())setPercentComplete:(f64)percent_complete {
    env.objc.borrow_mut(this).percent_complete = percent_complete.clamp(0.0, 100.0);
}

- (())reportAchievementWithCompletionHandler:(MutPtr<u8>)completion {
    // Local-only support. Game Center networking and account storage are
    // unavailable; report completion with no error so game progression works.
    finish_report(env, completion);
}

@end

};

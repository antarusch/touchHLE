/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Game Center achievements. Online Game Center storage is not emulated.

use crate::frameworks::foundation::ns_array;
use crate::mem::MutPtr;
use crate::objc::{id, objc_classes, ClassExports, nil};
use crate::cpu::GuestFunction;

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation GKAchievement: NSObject

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

@end

};

/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSNull`.

use crate::frameworks::foundation::ns_string;
use crate::objc::{id, msg, objc_classes, ClassExports, NSZonePtr, TrivialHostObject};

#[derive(Default)]
pub struct State {
    null: Option<id>,
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

// This is a singleton that takes the place of nil in collections which don't
// allow that value.
@implementation NSNull: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    msg![env; this null]
}

+ (id)null {
    if let Some(null) = env.framework_state.foundation.ns_null.null {
        null
    } else {
        let new = env.objc.alloc_static_object(
            this,
            Box::new(TrivialHostObject),
            &mut env.mem
        );
        env.framework_state.foundation.ns_null.null = Some(new);
        new
   }
}

- (id)retain { this }
- (id)initWithCoder:(id)_coder { this }
- (())encodeWithCoder:(id)_coder {}
- (())release {}
- (id)autorelease { this }

- (id)description {
    ns_string::get_static_str(env, "<null>")
}

- (id)debugDescription {
    msg![env; this description]
}

- (id)descriptionWithLocale:(id)_locale {
    msg![env; this description]
}

- (id)descriptionWithLocale:(id)_locale indent:(u32)_level {
    msg![env; this description]
}

- (id)copyWithZone:(NSZonePtr)_zone {
    this
}

@end

};

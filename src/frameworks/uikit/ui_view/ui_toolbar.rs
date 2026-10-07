/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIToolbar`.

use crate::objc::{id, objc_classes, ClassExports};

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIToolbar: UIView

- (())setItems:(id)_items {
    // Toolbar rendering is not implemented yet. Keeping the UIView behavior
    // allows apps to use a toolbar as an input accessory without crashing.
}

@end

};

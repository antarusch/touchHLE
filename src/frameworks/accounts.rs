/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Accounts framework compatibility constants.
//! The game can reference these notification names even if the actual
//! social-account services are not available in touchHLE.

use crate::dyld::{ConstantExports, HostConstant};

const CONSTANTS: ConstantExports = &[(
    "_ACAccountStoreDidChangeNotification",
    HostConstant::NSString("ACAccountStoreDidChangeNotification"),
)];

pub const DYLIB: crate::dyld::HostDylib = crate::dyld::HostDylib {
    path: "/System/Library/Frameworks/Accounts.framework/Accounts",
    aliases: &[],
    class_exports: &[],
    constant_exports: &[CONSTANTS],
    function_exports: &[],
};

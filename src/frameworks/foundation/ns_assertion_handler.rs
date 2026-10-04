/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Per-thread assertion handler and diagnostic failure reporting.
use super::ns_string::{to_rust_string, with_format};
use crate::objc::{id, msg, objc_classes, ClassExports, SEL};
#[derive(Default)]
pub(super) struct State {
    handler: id,
}
pub const CLASSES: ClassExports = objc_classes! {
(env, this, _cmd);
@implementation NSAssertionHandler: NSObject
+ (id)currentHandler {
    let existing = env
        .get_tl_framework_state()
        .foundation
        .ns_assertion_handler
        .handler;
    if existing != crate::objc::nil {
        return existing;
    }
    let handler: id = msg![env; this new];
    env.get_tl_framework_state()
        .foundation
        .ns_assertion_handler
        .handler = handler;
    handler
}
- (())handleFailureInMethod:(SEL)method object:(id)object file:(id)file lineNumber:(i32)line description:(id)description, ...args {
    let message = with_format(env, description, args.start());
    let file = to_rust_string(env, file);
    panic!(
        "Assertion failed in [{object:?} {}], {}:{line}: {message}",
        method.as_str(&env.mem),
        file
    );
}
- (())handleFailureInFunction:(id)function file:(id)file lineNumber:(i32)line description:(id)description, ...args {
    let message = with_format(env, description, args.start());
    panic!(
        "Assertion failed in {}, {}:{line}: {message}",
        to_rust_string(env, function),
        to_rust_string(env, file)
    );
}
@end
};

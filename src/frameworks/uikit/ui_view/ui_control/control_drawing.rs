/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Simple control primitives shared by sliders and segmented controls.
use crate::frameworks::core_graphics::cg_context::{CGContextFillRect, CGContextSetRGBFillColor};
use crate::frameworks::core_graphics::CGRect;
use crate::frameworks::uikit::ui_graphics::UIGraphicsGetCurrentContext;
use crate::Environment;
pub(super) fn fill(env: &mut Environment, rect: CGRect, color: (f32, f32, f32, f32)) {
    let context = UIGraphicsGetCurrentContext(env);
    CGContextSetRGBFillColor(env, context, color.0, color.1, color.2, color.3);
    CGContextFillRect(env, context, rect);
}

/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Mutable polyline paths and position sampling for keyframe animations.

use super::cg_affine_transform::CGAffineTransform;
use super::{CGFloat, CGPoint};
use crate::dyld::{export_c_func, FunctionExports};
use crate::frameworks::core_foundation::{CFRelease, CFRetain};
use crate::mem::ConstPtr;
use crate::objc::{id, objc_classes, ClassExports, HostObject, ObjC};
use crate::Environment;

#[derive(Default, Clone)]
pub(super) struct PathData {
    pub(super) subpaths: Vec<Vec<CGPoint>>,
}
impl HostObject for PathData {}
pub type CGPathRef = id;
pub const CLASSES: ClassExports = objc_classes! {
(env, this, _cmd);
@implementation _touchHLE_CGPath: NSObject
@end
};

fn CGPathCreateMutable(env: &mut Environment) -> CGPathRef {
    let class = env.objc.get_known_class("_touchHLE_CGPath", &mut env.mem);
    env.objc
        .alloc_object(class, Box::<PathData>::default(), &mut env.mem)
}
fn transformed(env: &Environment, t: ConstPtr<CGAffineTransform>, x: f32, y: f32) -> CGPoint {
    let point = CGPoint { x, y };
    if t.is_null() {
        point
    } else {
        env.mem.read(t).apply_to_point(point)
    }
}
fn CGPathMoveToPoint(
    env: &mut Environment,
    path: CGPathRef,
    transform: ConstPtr<CGAffineTransform>,
    x: CGFloat,
    y: CGFloat,
) {
    let point = transformed(env, transform, x, y);
    env.objc
        .borrow_mut::<PathData>(path)
        .subpaths
        .push(vec![point]);
}
fn CGPathAddLineToPoint(
    env: &mut Environment,
    path: CGPathRef,
    transform: ConstPtr<CGAffineTransform>,
    x: CGFloat,
    y: CGFloat,
) {
    let point = transformed(env, transform, x, y);
    let data = env.objc.borrow_mut::<PathData>(path);
    if let Some(subpath) = data.subpaths.last_mut() {
        subpath.push(point);
    } else {
        data.subpaths.push(vec![point]);
    }
}
fn CGPathCloseSubpath(env: &mut Environment, path: CGPathRef) {
    if let Some(points) = env.objc.borrow_mut::<PathData>(path).subpaths.last_mut() {
        if let Some(first) = points.first().copied() {
            points.push(first);
        }
    }
}
fn CGPathCreateCopy(env: &mut Environment, path: CGPathRef) -> CGPathRef {
    let data = env.objc.borrow::<PathData>(path).clone();
    let class = env.objc.get_known_class("_touchHLE_CGPath", &mut env.mem);
    env.objc.alloc_object(class, Box::new(data), &mut env.mem)
}
fn CGPathRetain(env: &mut Environment, path: CGPathRef) -> CGPathRef {
    CFRetain(env, path)
}
fn CGPathRelease(env: &mut Environment, path: CGPathRef) {
    CFRelease(env, path);
}

pub fn point_at(objc: &ObjC, path: CGPathRef, progress: f32) -> Option<CGPoint> {
    let data = objc.borrow::<PathData>(path);
    let segments: Vec<_> = data
        .subpaths
        .iter()
        .flat_map(|points| points.windows(2))
        .map(|pair| {
            let dx = pair[1].x - pair[0].x;
            let dy = pair[1].y - pair[0].y;
            (pair[0], pair[1], (dx * dx + dy * dy).sqrt())
        })
        .collect();
    let total: f32 = segments.iter().map(|(_, _, length)| length).sum();
    let mut distance = progress.clamp(0.0, 1.0) * total;
    for (a, b, length) in &segments {
        if distance <= *length && *length > 0.0 {
            return Some(*a + (*b - *a) * (distance / length));
        }
        distance -= length;
    }
    data.subpaths
        .last()
        .and_then(|points| points.last())
        .copied()
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(CGPathCreateMutable()),
    export_c_func!(CGPathMoveToPoint(_, _, _, _)),
    export_c_func!(CGPathAddLineToPoint(_, _, _, _)),
    export_c_func!(CGPathCloseSubpath(_)),
    export_c_func!(CGPathCreateCopy(_)),
    export_c_func!(CGPathRetain(_)),
    export_c_func!(CGPathRelease(_)),
];

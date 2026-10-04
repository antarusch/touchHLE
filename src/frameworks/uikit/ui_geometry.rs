//! `UIGeometry.h`
//!
//! See also [crate::frameworks::core_graphics::cg_geometry].

use crate::dyld::{export_c_func, FunctionExports};
use crate::frameworks::core_graphics::{CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::ns_string;
use crate::objc::{autorelease, id};
use crate::Environment;

#[derive(Copy, Clone, Debug, Default, PartialEq)]
#[repr(C)]
pub struct UIEdgeInsets {
    pub top: f32,
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
}
unsafe impl crate::mem::SafeRead for UIEdgeInsets {}
impl crate::abi::GuestArg for UIEdgeInsets {
    const REG_COUNT: usize = 4;
    fn from_regs(regs: &[u32]) -> Self {
        Self {
            top: f32::from_bits(regs[0]),
            left: f32::from_bits(regs[1]),
            bottom: f32::from_bits(regs[2]),
            right: f32::from_bits(regs[3]),
        }
    }
    fn to_regs(self, regs: &mut [u32]) {
        for (dst, value) in regs
            .iter_mut()
            .zip([self.top, self.left, self.bottom, self.right])
        {
            *dst = value.to_bits();
        }
    }
}
crate::abi::impl_GuestRet_for_large_struct!(UIEdgeInsets);

// Apple's documentation says all of these return zeroes if the input is not
// well-formed.
pub fn CGPointFromString(env: &mut Environment, string: id) -> CGPoint {
    // TODO: avoid copy
    ns_string::to_rust_string(env, string)
        .parse()
        .unwrap_or_default()
}
pub fn CGSizeFromString(env: &mut Environment, string: id) -> CGSize {
    // TODO: avoid copy
    ns_string::to_rust_string(env, string)
        .parse()
        .unwrap_or_default()
}
pub fn CGRectFromString(env: &mut Environment, string: id) -> CGRect {
    // TODO: avoid copy
    ns_string::to_rust_string(env, string)
        .parse()
        .unwrap_or_default()
}

pub fn NSStringFromCGPoint(env: &mut Environment, point: CGPoint) -> id {
    let s = ns_string::from_rust_string(env, point.to_string());
    autorelease(env, s)
}
pub fn NSStringFromCGSize(env: &mut Environment, size: CGSize) -> id {
    let s = ns_string::from_rust_string(env, size.to_string());
    autorelease(env, s)
}
pub fn NSStringFromCGRect(env: &mut Environment, rect: CGRect) -> id {
    let s = ns_string::from_rust_string(env, rect.to_string());
    autorelease(env, s)
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(CGPointFromString(_)),
    export_c_func!(CGSizeFromString(_)),
    export_c_func!(CGRectFromString(_)),
    export_c_func!(NSStringFromCGPoint(_)),
    export_c_func!(NSStringFromCGSize(_)),
    export_c_func!(NSStringFromCGRect(_)),
];

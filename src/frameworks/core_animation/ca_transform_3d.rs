/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Core Animation's homogeneous, row-vector transform ABI.

use crate::abi::{impl_GuestRet_for_large_struct, GuestArg};
use crate::dyld::{export_c_func, ConstantExports, FunctionExports, HostConstant};
use crate::frameworks::core_graphics::cg_affine_transform::CGAffineTransform;
use crate::frameworks::core_graphics::CGFloat;
use crate::matrix::Matrix;
use crate::mem::SafeRead;
use crate::Environment;
use std::ops::{Add, Mul, Sub};

#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(C)]
pub struct CATransform3D {
    // ABI order: m11, m12, ..., m14, m21, ..., m44.
    pub elements: [CGFloat; 16],
}
unsafe impl SafeRead for CATransform3D {}
impl GuestArg for CATransform3D {
    const REG_COUNT: usize = 16;
    fn from_regs(regs: &[u32]) -> Self {
        Self {
            elements: std::array::from_fn(|i| f32::from_bits(regs[i])),
        }
    }
    fn to_regs(self, regs: &mut [u32]) {
        for (dst, value) in regs.iter_mut().zip(self.elements) {
            *dst = value.to_bits();
        }
    }
}
impl_GuestRet_for_large_struct!(CATransform3D);

pub const CATransform3DIdentity: CATransform3D = CATransform3D {
    elements: [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ],
};
impl Default for CATransform3D {
    fn default() -> Self {
        CATransform3DIdentity
    }
}
impl From<CATransform3D> for Matrix<4> {
    fn from(t: CATransform3D) -> Self {
        Matrix::from_columns(std::array::from_fn(|i| {
            std::array::from_fn(|j| t.elements[i * 4 + j])
        }))
    }
}
impl From<Matrix<4>> for CATransform3D {
    fn from(t: Matrix<4>) -> Self {
        Self {
            elements: std::array::from_fn(|i| t.columns()[i / 4][i % 4]),
        }
    }
}
impl From<CGAffineTransform> for CATransform3D {
    fn from(t: CGAffineTransform) -> Self {
        <Matrix<4> as From<_>>::from(t).into()
    }
}
impl CATransform3D {
    pub fn affine(self) -> CGAffineTransform {
        let m = self.elements;
        CGAffineTransform {
            a: m[0],
            b: m[1],
            c: m[4],
            d: m[5],
            tx: m[12],
            ty: m[13],
        }
    }
    pub fn scale(x: f32, y: f32, z: f32) -> Self {
        let mut t = Self::default();
        t.elements[0] = x;
        t.elements[5] = y;
        t.elements[10] = z;
        t
    }
    pub fn translation(x: f32, y: f32, z: f32) -> Self {
        let mut t = Self::default();
        t.elements[12] = x;
        t.elements[13] = y;
        t.elements[14] = z;
        t
    }
    pub fn rotation(angle: f32, x: f32, y: f32, z: f32) -> Self {
        let length = (x * x + y * y + z * z).sqrt();
        if length == 0.0 {
            return Self::default();
        }
        let (x, y, z) = (x / length, y / length, z / length);
        let (s, c) = angle.sin_cos();
        let v = 1.0 - c;
        Self {
            elements: [
                c + x * x * v,
                y * x * v + z * s,
                z * x * v - y * s,
                0.0,
                x * y * v - z * s,
                c + y * y * v,
                z * y * v + x * s,
                0.0,
                x * z * v + y * s,
                y * z * v - x * s,
                c + z * z * v,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
            ],
        }
    }
    pub fn concat(self, other: Self) -> Self {
        <Matrix<4> as From<_>>::from(self)
            .multiply(&other.into())
            .into()
    }

    #[allow(clippy::needless_range_loop)]
    pub fn inverse(self) -> Self {
        let matrix: Matrix<4> = self.into();
        let mut augmented = [[0.0f32; 8]; 4];

        // Matrix stores columns internally; build a conventional row-major
        // augmented matrix [M | I] for Gauss-Jordan elimination.
        for row in 0..4 {
            for column in 0..4 {
                augmented[row][column] = matrix.columns()[column][row];
            }
            augmented[row][row + 4] = 1.0;
        }

        for column in 0..4 {
            let mut pivot_row = column;
            for row in (column + 1)..4 {
                if augmented[row][column].abs() > augmented[pivot_row][column].abs() {
                    pivot_row = row;
                }
            }

            if augmented[pivot_row][column] == 0.0 {
                // QuartzCore returns the original transform when no inverse
                // exists.
                return self;
            }

            if pivot_row != column {
                augmented.swap(pivot_row, column);
            }

            let pivot = augmented[column][column];
            for entry in &mut augmented[column] {
                *entry /= pivot;
            }

            for row in 0..4 {
                if row == column {
                    continue;
                }
                let factor = augmented[row][column];
                if factor == 0.0 {
                    continue;
                }
                for index in 0..8 {
                    augmented[row][index] -= factor * augmented[column][index];
                }
            }
        }

        let inverse = Matrix::from_columns(std::array::from_fn(|column| {
            std::array::from_fn(|row| augmented[row][column + 4])
        }));
        inverse.into()
    }
}
impl Add for CATransform3D {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self {
            elements: std::array::from_fn(|i| self.elements[i] + other.elements[i]),
        }
    }
}
impl Sub for CATransform3D {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self {
            elements: std::array::from_fn(|i| self.elements[i] - other.elements[i]),
        }
    }
}
impl Mul<f32> for CATransform3D {
    type Output = Self;
    fn mul(self, value: f32) -> Self {
        Self {
            elements: self.elements.map(|x| x * value),
        }
    }
}

fn CATransform3DMakeScale(_: &mut Environment, x: f32, y: f32, z: f32) -> CATransform3D {
    CATransform3D::scale(x, y, z)
}
fn CATransform3DMakeTranslation(_: &mut Environment, x: f32, y: f32, z: f32) -> CATransform3D {
    CATransform3D::translation(x, y, z)
}
fn CATransform3DMakeRotation(
    _: &mut Environment,
    angle: f32,
    x: f32,
    y: f32,
    z: f32,
) -> CATransform3D {
    CATransform3D::rotation(angle, x, y, z)
}
fn CATransform3DConcat(_: &mut Environment, a: CATransform3D, b: CATransform3D) -> CATransform3D {
    a.concat(b)
}
fn CATransform3DInvert(_: &mut Environment, t: CATransform3D) -> CATransform3D {
    t.inverse()
}
fn CATransform3DScale(
    _: &mut Environment,
    t: CATransform3D,
    x: f32,
    y: f32,
    z: f32,
) -> CATransform3D {
    CATransform3D::scale(x, y, z).concat(t)
}
fn CATransform3DTranslate(
    _: &mut Environment,
    t: CATransform3D,
    x: f32,
    y: f32,
    z: f32,
) -> CATransform3D {
    CATransform3D::translation(x, y, z).concat(t)
}
fn CATransform3DRotate(
    _: &mut Environment,
    t: CATransform3D,
    angle: f32,
    x: f32,
    y: f32,
    z: f32,
) -> CATransform3D {
    CATransform3D::rotation(angle, x, y, z).concat(t)
}
fn CATransform3DIsIdentity(_: &mut Environment, t: CATransform3D) -> bool {
    t == CATransform3DIdentity
}
pub const CONSTANTS: ConstantExports = &[(
    "_CATransform3DIdentity",
    HostConstant::Custom(|env| {
        env.mem
            .alloc_and_write(CATransform3DIdentity)
            .cast()
            .cast_const()
    }),
)];
pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(CATransform3DMakeScale(_, _, _)),
    export_c_func!(CATransform3DMakeTranslation(_, _, _)),
    export_c_func!(CATransform3DMakeRotation(_, _, _, _)),
    export_c_func!(CATransform3DConcat(_, _)),
    export_c_func!(CATransform3DInvert(_)),
    export_c_func!(CATransform3DScale(_, _, _, _)),
    export_c_func!(CATransform3DTranslate(_, _, _, _)),
    export_c_func!(CATransform3DRotate(_, _, _, _, _)),
    export_c_func!(CATransform3DIsIdentity(_)),
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concat_and_axis_rotation() {
        let t =
            CATransform3D::scale(2.0, 3.0, 4.0).concat(CATransform3D::translation(5.0, 6.0, 7.0));
        assert_eq!(
            <Matrix<4> as From<_>>::from(t).transform([1.0, 1.0, 1.0, 1.0]),
            [7.0, 9.0, 11.0, 1.0]
        );
        let r = CATransform3D::rotation(std::f32::consts::FRAC_PI_2, 0.0, 0.0, 2.0);
        let p = <Matrix<4> as From<_>>::from(r).transform([1.0, 0.0, 0.0, 1.0]);
        assert!(p[0].abs() < 1e-6 && (p[1] - 1.0).abs() < 1e-6);
    }
    #[test]
    fn guest_transform_round_trip() {
        let t = CATransform3D::translation(12.0, -3.0, 4.0);
        let mut words = [0; 16];
        t.to_regs(&mut words);
        assert_eq!(CATransform3D::from_regs(&words), t);
        assert_eq!(std::mem::size_of::<CATransform3D>(), 64);
    }

    #[test]
    fn inverse_round_trip_and_singular_behavior() {
        let t = CATransform3D::scale(2.0, 3.0, 4.0)
            .concat(CATransform3D::rotation(0.37, 0.5, 1.0, -0.25))
            .concat(CATransform3D::translation(5.0, -7.0, 9.0));
        let product = t.concat(t.inverse());
        for (actual, expected) in product
            .elements
            .iter()
            .zip(CATransform3DIdentity.elements.iter())
        {
            assert!((actual - expected).abs() < 1e-4, "{actual} != {expected}");
        }

        let singular = CATransform3D::scale(1.0, 0.0, 2.0);
        assert_eq!(singular.inverse(), singular);
    }
}

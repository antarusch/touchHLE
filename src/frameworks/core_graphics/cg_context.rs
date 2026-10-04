/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `CGContext.h`

use super::cg_affine_transform::{CGAffineTransform, CGAffineTransformIdentity};
use super::cg_bitmap_context::{
    CGBitmapContextDrawer, CGBitmapContextGetHeight, CGBitmapContextGetWidth,
};
use super::cg_color::CGColorRef;
use super::cg_color_space::{
    kCGColorSpaceModelMonochrome, kCGColorSpaceModelRGB, CGColorSpaceGetModel, CGColorSpaceRef,
};
use super::cg_font::{CGFontHostObject, CGFontRef, CGFontRelease, CGFontRetain, CGGlyph};
use super::cg_geometry::CGPointZero;
use super::cg_image::CGImageRef;
use super::{cg_bitmap_context, cg_color, CGFloat, CGPoint, CGRect, CGSize};
use crate::dyld::{export_c_func, FunctionExports};
use crate::frameworks::core_foundation::{CFRelease, CFRetain, CFTypeRef};
use crate::frameworks::uikit;
use crate::mem::{ConstPtr, GuestUSize};
use crate::objc::{objc_classes, ClassExports, HostObject};
use crate::Environment;

type CGInterpolationQuality = i32;

type CGTextDrawingMode = i32;
const kCGTextFill: CGTextDrawingMode = 0;
const kCGTextFillStroke: CGTextDrawingMode = 2;

pub type CGBlendMode = i32;
pub const kCGBlendModeNormal: CGBlendMode = 0;
pub const kCGBlendModeMultiply: CGBlendMode = 1;
pub const kCGBlendModeScreen: CGBlendMode = 2;
#[allow(unused)]
pub const kCGBlendModeOverlay: CGBlendMode = 3;
pub const kCGBlendModeDarken: CGBlendMode = 4;
pub const kCGBlendModeLighten: CGBlendMode = 5;
pub const kCGBlendModeCopy: CGBlendMode = 17;

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

// CGContext seems to be a CFType-based type, but in our implementation those
// are just Objective-C types, so we need a class for it, but its name is not
// visible anywhere.
@implementation _touchHLE_CGContext: NSObject

- (())dealloc {
    let host_obj = env.objc.borrow::<CGContextHostObject>(this);
    let CGContextSubclass::CGBitmapContext(bitmap_data) = host_obj.subclass;
    if bitmap_data.data_is_owned {
        env.mem.free(bitmap_data.data);
    }
    CGFontRelease(env, host_obj.font);

    env.objc.dealloc_object(this, &mut env.mem)
}

@end

};

// TODO: keep more states saved once they are implemented
type ContextState = (
    (CGFloat, CGFloat, CGFloat, CGFloat), // RGB fill color
    CGAffineTransform,                    // transform
    CGFontRef,                            // font
    CGFloat,                              // font size
    CGBlendMode,                          // blend mode
    CGFloat,                              // line width
    (CGFloat, CGFloat, CGFloat, CGFloat), // RGB stroke color
    i32,                                  // line cap
    (CGFloat, Vec<CGFloat>),              // dash phase and pattern
);

pub(super) struct CGContextHostObject {
    pub(super) subclass: CGContextSubclass,
    pub(super) rgb_fill_color: (CGFloat, CGFloat, CGFloat, CGFloat),
    pub(super) rgb_stroke_color: (CGFloat, CGFloat, CGFloat, CGFloat),
    pub(super) line_width: CGFloat,
    pub(super) line_cap: i32,
    pub(super) line_dash: (CGFloat, Vec<CGFloat>),
    pub(super) path: super::cg_path::PathData,
    pub(super) font: CGFontRef,
    pub(super) font_size: CGFloat,
    /// Current transform.
    pub(super) transform: CGAffineTransform,
    pub(super) blend_mode: CGBlendMode,
    /// Text transform.
    pub(super) text_transform: Option<CGAffineTransform>,
    pub(super) state_stack: Vec<ContextState>,
}
impl HostObject for CGContextHostObject {}

pub(super) enum CGContextSubclass {
    CGBitmapContext(cg_bitmap_context::CGBitmapContextData),
}

pub type CGContextRef = CFTypeRef;

pub fn CGContextRelease(env: &mut Environment, c: CGContextRef) {
    if !c.is_null() {
        CFRelease(env, c);
    }
}
pub fn CGContextRetain(env: &mut Environment, c: CGContextRef) -> CGContextRef {
    if !c.is_null() {
        CFRetain(env, c)
    } else {
        c
    }
}

fn CGContextSetBlendMode(env: &mut Environment, context: CGContextRef, blend_mode: CGBlendMode) {
    env.objc
        .borrow_mut::<CGContextHostObject>(context)
        .blend_mode = blend_mode;
}

fn CGContextSetFillColorSpace(
    env: &mut Environment,
    _context: CGContextRef,
    space: CGColorSpaceRef,
) {
    let color_model = CGColorSpaceGetModel(env, space);
    assert!(color_model == kCGColorSpaceModelMonochrome || color_model == kCGColorSpaceModelRGB);
    // TODO
}

fn CGContextSetFillColorWithColor(env: &mut Environment, context: CGContextRef, color: CGColorRef) {
    let (r, g, b, a) = cg_color::to_rgba(&env.objc, color);
    CGContextSetRGBFillColor(env, context, r, g, b, a)
}

pub fn CGContextSetRGBFillColor(
    env: &mut Environment,
    context: CGContextRef,
    red: CGFloat,
    green: CGFloat,
    blue: CGFloat,
    alpha: CGFloat,
) {
    let color = (red, green, blue, alpha);
    env.objc
        .borrow_mut::<CGContextHostObject>(context)
        .rgb_fill_color = color;
}

fn CGContextSetGrayFillColor(
    env: &mut Environment,
    context: CGContextRef,
    gray: CGFloat,
    alpha: CGFloat,
) {
    let color = (gray, gray, gray, alpha);
    env.objc
        .borrow_mut::<CGContextHostObject>(context)
        .rgb_fill_color = color;
}

fn CGContextSetGrayStrokeColor(
    env: &mut Environment,
    context: CGContextRef,
    gray: CGFloat,
    alpha: CGFloat,
) {
    CGContextSetRGBStrokeColor(env, context, gray, gray, gray, alpha);
}
pub fn CGContextSetRGBStrokeColor(
    env: &mut Environment,
    context: CGContextRef,
    r: CGFloat,
    g: CGFloat,
    b: CGFloat,
    a: CGFloat,
) {
    env.objc
        .borrow_mut::<CGContextHostObject>(context)
        .rgb_stroke_color = (r, g, b, a);
}

fn CGContextSetStrokeColorWithColor(
    env: &mut Environment,
    context: CGContextRef,
    color: CGColorRef,
) {
    let (r, g, b, a) = cg_color::to_rgba(&env.objc, color);
    CGContextSetRGBStrokeColor(env, context, r, g, b, a);
}

fn CGContextSetLineWidth(env: &mut Environment, context: CGContextRef, width: CGFloat) {
    // Quartz specifies a positive width, in user-space units.
    if width.is_finite() && width > 0.0 {
        env.objc
            .borrow_mut::<CGContextHostObject>(context)
            .line_width = width;
    }
}

fn CGContextStrokeRect(env: &mut Environment, context: CGContextRef, rect: CGRect) {
    cg_bitmap_context::stroke_rect(env, context, rect, None);
}

fn CGContextStrokeRectWithWidth(
    env: &mut Environment,
    context: CGContextRef,
    rect: CGRect,
    width: CGFloat,
) {
    cg_bitmap_context::stroke_rect(env, context, rect, Some(width));
}

fn CGContextSetShadowWithColor(
    _env: &mut Environment,
    context: CGContextRef,
    offset: CGSize,
    blur: CGFloat,
    color: CGColorRef,
) {
    log!(
        "TODO: CGContextSetShadowWithColor({:?}, {}, {}, {:?})",
        context,
        offset,
        blur,
        color
    );
}

pub fn CGContextFillRect(env: &mut Environment, context: CGContextRef, rect: CGRect) {
    cg_bitmap_context::fill_rect(env, context, rect, /* clear: */ false);
}

pub fn CGContextClearRect(env: &mut Environment, context: CGContextRef, rect: CGRect) {
    cg_bitmap_context::fill_rect(env, context, rect, /* clear: */ true);
}

fn CGContextClipToRect(env: &mut Environment, context: CGContextRef, rect: CGRect) {
    if rect.origin == CGPointZero
        && rect.size.height == CGBitmapContextGetHeight(env, context) as f32
        && rect.size.width == CGBitmapContextGetWidth(env, context) as f32
    {
        assert!(env
            .objc
            .borrow_mut::<CGContextHostObject>(context)
            .transform
            .is_identity());
        // All good, clipping is not needed!
        return;
    }
    todo!();
}

pub fn CGContextConcatCTM(
    env: &mut Environment,
    context: CGContextRef,
    transform: CGAffineTransform,
) {
    log_dbg!("CGContextConcatCTM({:?})", transform);
    let host_obj = env.objc.borrow_mut::<CGContextHostObject>(context);
    host_obj.transform = transform.concat(host_obj.transform);
}
pub fn CGContextGetCTM(env: &mut Environment, context: CGContextRef) -> CGAffineTransform {
    let res = env.objc.borrow::<CGContextHostObject>(context).transform;
    log_dbg!("CGContextGetCTM() => {:?}", res);
    res
}
pub fn CGContextRotateCTM(env: &mut Environment, context: CGContextRef, angle: CGFloat) {
    log_dbg!("CGContextRotateCTM({:?})", angle);
    let host_obj = env.objc.borrow_mut::<CGContextHostObject>(context);
    host_obj.transform = host_obj.transform.rotate(angle);
}
pub fn CGContextScaleCTM(env: &mut Environment, context: CGContextRef, x: CGFloat, y: CGFloat) {
    log_dbg!("CGContextScaleCTM({:?})", (x, y));
    let host_obj = env.objc.borrow_mut::<CGContextHostObject>(context);
    host_obj.transform = host_obj.transform.scale(x, y);
}
pub fn CGContextTranslateCTM(
    env: &mut Environment,
    context: CGContextRef,
    tx: CGFloat,
    ty: CGFloat,
) {
    log_dbg!("CGContextTranslateCTM({:?})", (tx, ty));
    let host_obj = env.objc.borrow_mut::<CGContextHostObject>(context);
    host_obj.transform = host_obj.transform.translate(tx, ty);
}

pub fn CGContextDrawImage(
    env: &mut Environment,
    context: CGContextRef,
    rect: CGRect,
    image: CGImageRef,
) {
    cg_bitmap_context::draw_image(env, context, rect, image);
}

fn CGContextSaveGState(env: &mut Environment, context: CGContextRef) {
    let host_obj = env.objc.borrow_mut::<CGContextHostObject>(context);
    host_obj.state_stack.push((
        host_obj.rgb_fill_color,
        host_obj.transform,
        host_obj.font,
        host_obj.font_size,
        host_obj.blend_mode,
        host_obj.line_width,
        host_obj.rgb_stroke_color,
        host_obj.line_cap,
        host_obj.line_dash.clone(),
    ));
    CGFontRetain(env, env.objc.borrow::<CGContextHostObject>(context).font);
}

fn CGContextRestoreGState(env: &mut Environment, context: CGContextRef) {
    // We need to release _old_ font, there are 2 cases:
    // - font hasn't been set between save/restore -> this release corresponds
    // the font retain from save
    // - font has been set between save/restore -> we need to release old font
    // retained on the set
    CGFontRelease(env, env.objc.borrow::<CGContextHostObject>(context).font);
    let host_obj = env.objc.borrow_mut::<CGContextHostObject>(context);
    let state = host_obj.state_stack.pop().unwrap();
    host_obj.rgb_fill_color = state.0;
    host_obj.transform = state.1;
    host_obj.font = state.2;
    host_obj.font_size = state.3;
    host_obj.blend_mode = state.4;
    host_obj.line_width = state.5;
    host_obj.rgb_stroke_color = state.6;
    host_obj.line_cap = state.7;
    host_obj.line_dash = state.8;
}

fn CGContextBeginPath(env: &mut Environment, context: CGContextRef) {
    env.objc
        .borrow_mut::<CGContextHostObject>(context)
        .path
        .subpaths
        .clear();
}
fn CGContextMoveToPoint(env: &mut Environment, context: CGContextRef, x: CGFloat, y: CGFloat) {
    let host = env.objc.borrow_mut::<CGContextHostObject>(context);
    let point = host.transform.apply_to_point(CGPoint { x, y });
    host.path.subpaths.push(vec![point]);
}
fn CGContextAddLineToPoint(env: &mut Environment, context: CGContextRef, x: CGFloat, y: CGFloat) {
    let host = env.objc.borrow_mut::<CGContextHostObject>(context);
    let point = host.transform.apply_to_point(CGPoint { x, y });
    if let Some(points) = host.path.subpaths.last_mut() {
        points.push(point);
    } else {
        host.path.subpaths.push(vec![point]);
    }
}
fn CGContextAddLines(
    env: &mut Environment,
    context: CGContextRef,
    points: ConstPtr<CGPoint>,
    count: GuestUSize,
) {
    for i in 0..count {
        let point = env.mem.read(points + i);
        if i == 0 {
            CGContextMoveToPoint(env, context, point.x, point.y);
        } else {
            CGContextAddLineToPoint(env, context, point.x, point.y);
        }
    }
}
fn CGContextClosePath(env: &mut Environment, context: CGContextRef) {
    if let Some(points) = env
        .objc
        .borrow_mut::<CGContextHostObject>(context)
        .path
        .subpaths
        .last_mut()
    {
        if let Some(first) = points.first().copied() {
            points.push(first);
        }
    }
}
fn CGContextSetLineCap(env: &mut Environment, context: CGContextRef, cap: i32) {
    env.objc.borrow_mut::<CGContextHostObject>(context).line_cap = cap;
}
fn CGContextSetLineDash(
    env: &mut Environment,
    context: CGContextRef,
    phase: CGFloat,
    lengths: ConstPtr<CGFloat>,
    count: GuestUSize,
) {
    let lengths: Vec<_> = (0..count).map(|i| env.mem.read(lengths + i)).collect();
    assert!(lengths
        .iter()
        .all(|length| length.is_finite() && *length >= 0.0));
    env.objc
        .borrow_mut::<CGContextHostObject>(context)
        .line_dash = (phase, lengths);
}
fn CGContextStrokePath(env: &mut Environment, context: CGContextRef) {
    cg_bitmap_context::stroke_path(env, context);
}

#[cfg(test)]
pub(crate) fn integration_check(env: &mut Environment) {
    use super::cg_color_space::{CGColorSpaceCreateDeviceRGB, CGColorSpaceRelease};
    use super::cg_image::kCGImageAlphaPremultipliedLast;
    let color_space = CGColorSpaceCreateDeviceRGB(env);
    let pixels = env.mem.calloc(16 * 8 * 4);
    let context = cg_bitmap_context::CGBitmapContextCreate(
        env,
        pixels,
        16,
        8,
        8,
        64,
        color_space,
        kCGImageAlphaPremultipliedLast,
    );
    CGContextSetRGBStrokeColor(env, context, 1.0, 0.0, 0.0, 1.0);
    CGContextSetLineWidth(env, context, 2.0);
    let lengths: crate::mem::MutPtr<f32> = env.mem.alloc(8).cast();
    env.mem.write(lengths, 4.0);
    env.mem.write(lengths + 1, 4.0);
    CGContextSetLineDash(env, context, 0.0, lengths.cast().cast_const(), 2);
    CGContextSaveGState(env, context);
    CGContextSetLineDash(env, context, 0.0, ConstPtr::null(), 0);
    CGContextRestoreGState(env, context);
    CGContextBeginPath(env, context);
    CGContextMoveToPoint(env, context, 1.0, 4.0);
    CGContextAddLineToPoint(env, context, 15.0, 4.0);
    CGContextStrokePath(env, context);
    let data = env.mem.bytes_at(pixels.cast(), 16 * 8 * 4);
    let alpha = |x: usize| data[(3 * 16 + x) * 4 + 3];
    assert_eq!(alpha(2), 255);
    assert_eq!(alpha(6), 0);
    assert_eq!(alpha(10), 255);
    assert!(env
        .objc
        .borrow::<CGContextHostObject>(context)
        .path
        .subpaths
        .is_empty());
    CGContextRelease(env, context);
    CGColorSpaceRelease(env, color_space);
    for pointer in [pixels, lengths.cast::<std::ffi::c_void>()] {
        env.mem.free(pointer);
    }
}

fn CGContextSetInterpolationQuality(
    _env: &mut Environment,
    context: CGContextRef,
    quality: CGInterpolationQuality,
) {
    log!(
        "TODO: CGContextSetInterpolationQuality({:?}, {:?})",
        context,
        quality
    );
}
fn CGContextSetAllowsAntialiasing(_env: &mut Environment, context: CGContextRef, allow: bool) {
    log!(
        "TODO: CGContextSetAllowsAntialiasing({:?}, {})",
        context,
        allow
    );
}

fn CGContextSetShouldSmoothFonts(_env: &mut Environment, context: CGContextRef, should: bool) {
    log!(
        "TODO: CGContextSetShouldSmoothFonts({:?}, {})",
        context,
        should
    );
}

fn CGContextSetFont(env: &mut Environment, context: CGContextRef, font: CGFontRef) {
    CGFontRetain(env, font);
    let old_font = env.objc.borrow_mut::<CGContextHostObject>(context).font;
    CGFontRelease(env, old_font);
    env.objc.borrow_mut::<CGContextHostObject>(context).font = font;
}

fn CGContextSetFontSize(env: &mut Environment, context: CGContextRef, size: CGFloat) {
    env.objc
        .borrow_mut::<CGContextHostObject>(context)
        .font_size = size;
}

fn CGContextSetTextDrawingMode(
    _env: &mut Environment,
    _context: CGContextRef,
    mode: CGTextDrawingMode,
) {
    assert!(mode == kCGTextFill || mode == kCGTextFillStroke); // TODO: support other modes
}

fn CGContextSetTextMatrix(
    env: &mut Environment,
    context: CGContextRef,
    transform: CGAffineTransform,
) {
    log_dbg!("CGContextSetTextMatrix({:?})", transform);
    env.objc
        .borrow_mut::<CGContextHostObject>(context)
        .text_transform = Some(transform);
}

fn CGContextShowGlyphsAtPoint(
    env: &mut Environment,
    context: CGContextRef,
    x: CGFloat,
    y: CGFloat,
    glyphs: ConstPtr<CGGlyph>,
    count: GuestUSize,
) {
    let mut glyph_ids = Vec::new();
    for i in 0..count {
        let glyph_id = env.mem.read(glyphs + i);
        glyph_ids.push(rusttype::GlyphId(glyph_id));
    }

    let font = env.objc.borrow::<CGContextHostObject>(context).font;
    let font_size = env.objc.borrow::<CGContextHostObject>(context).font_size;
    let text_transform = env
        .objc
        .borrow::<CGContextHostObject>(context)
        .text_transform
        .unwrap_or(CGAffineTransformIdentity);

    let font = &env.objc.borrow::<CGFontHostObject>(font).font;

    let mut drawer = CGBitmapContextDrawer::new(&env.objc, &mut env.mem, context);
    let fill_color = drawer.rgb_fill_color();

    font.draw_glyphs(
        font_size,
        glyph_ids,
        (x, y),
        text_transform,
        |raster_glyph| {
            uikit::ui_font::draw_font_glyph(
                &mut drawer,
                raster_glyph,
                fill_color,
                /* clip_x: */ None,
                /* clip_y: */ None,
            )
        },
    );
}

fn CGContextShowGlyphsAtPositions(
    env: &mut Environment,
    context: CGContextRef,
    glyphs: ConstPtr<CGGlyph>,
    positions: ConstPtr<CGPoint>,
    count: GuestUSize,
) {
    let text_transform = env
        .objc
        .borrow::<CGContextHostObject>(context)
        .text_transform
        .unwrap_or(CGAffineTransformIdentity);
    assert!(text_transform.tx == 0.0 && text_transform.ty == 0.0); // TODO

    for i in 0..count {
        let glyph_ptr = glyphs + i;
        let point = env.mem.read(positions + i);
        let transformed_point = text_transform.apply_to_point(point);
        CGContextShowGlyphsAtPoint(
            env,
            context,
            transformed_point.x,
            transformed_point.y,
            glyph_ptr,
            1,
        );
    }
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(CGContextBeginPath(_)),
    export_c_func!(CGContextMoveToPoint(_, _, _)),
    export_c_func!(CGContextAddLineToPoint(_, _, _)),
    export_c_func!(CGContextAddLines(_, _, _)),
    export_c_func!(CGContextClosePath(_)),
    export_c_func!(CGContextSetLineCap(_, _)),
    export_c_func!(CGContextSetLineDash(_, _, _, _)),
    export_c_func!(CGContextStrokePath(_)),
    export_c_func!(CGContextRetain(_)),
    export_c_func!(CGContextRelease(_)),
    export_c_func!(CGContextSetBlendMode(_, _)),
    export_c_func!(CGContextSetFillColorSpace(_, _)),
    export_c_func!(CGContextSetFillColorWithColor(_, _)),
    export_c_func!(CGContextSetRGBFillColor(_, _, _, _, _)),
    export_c_func!(CGContextSetGrayFillColor(_, _, _)),
    export_c_func!(CGContextSetGrayStrokeColor(_, _, _)),
    export_c_func!(CGContextSetRGBStrokeColor(_, _, _, _, _)),
    export_c_func!(CGContextSetStrokeColorWithColor(_, _)),
    export_c_func!(CGContextSetLineWidth(_, _)),
    export_c_func!(CGContextStrokeRect(_, _)),
    export_c_func!(CGContextStrokeRectWithWidth(_, _, _)),
    export_c_func!(CGContextSetShadowWithColor(_, _, _, _)),
    export_c_func!(CGContextFillRect(_, _)),
    export_c_func!(CGContextClearRect(_, _)),
    export_c_func!(CGContextClipToRect(_, _)),
    export_c_func!(CGContextConcatCTM(_, _)),
    export_c_func!(CGContextGetCTM(_)),
    export_c_func!(CGContextRotateCTM(_, _)),
    export_c_func!(CGContextScaleCTM(_, _, _)),
    export_c_func!(CGContextTranslateCTM(_, _, _)),
    export_c_func!(CGContextDrawImage(_, _, _)),
    export_c_func!(CGContextSaveGState(_)),
    export_c_func!(CGContextRestoreGState(_)),
    export_c_func!(CGContextSetInterpolationQuality(_, _)),
    export_c_func!(CGContextSetAllowsAntialiasing(_, _)),
    export_c_func!(CGContextSetShouldSmoothFonts(_, _)),
    export_c_func!(CGContextSetFont(_, _)),
    export_c_func!(CGContextSetFontSize(_, _)),
    export_c_func!(CGContextSetTextDrawingMode(_, _)),
    export_c_func!(CGContextSetTextMatrix(_, _)),
    export_c_func!(CGContextShowGlyphsAtPoint(_, _, _, _, _)),
    export_c_func!(CGContextShowGlyphsAtPositions(_, _, _, _)),
];

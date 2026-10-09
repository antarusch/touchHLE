/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Native OpenGL ES 2.0 backend.
//! The legacy GLES trait remains available for shared framebuffer/texture
//! operations. ES 1.x-only fixed-function calls are not emulated here.

use super::gles20_raw as gles20;
use super::gles11_raw::types::*;
use super::{GLES, GLESContext};
use crate::window::{GLContext, GLVersion, Window};
use std::ffi::CStr;
use std::marker::PhantomData;

pub struct GLES2NativeContext {
    gl_ctx: GLContext,
    is_loaded: bool,
}
impl GLESContext for GLES2NativeContext {
    fn description() -> &'static str { "Native OpenGL ES 2.0" }
    fn new(window: &mut Window) -> Result<Self, String> {
        Ok(Self { gl_ctx: window.create_gl_context(GLVersion::GLES20)?, is_loaded: false })
    }
    fn make_current<'gl_ctx, 'win: 'gl_ctx>(
        &'gl_ctx mut self, window: &'win mut Window,
    ) -> Box<dyn GLES + 'gl_ctx> {
        if !self.gl_ctx.is_current() || !self.is_loaded {
            unsafe { window.make_gl_context_current(&self.gl_ctx); }
            gles20::load_with(|symbol| window.gl_get_proc_address(symbol));
            self.is_loaded = true;
        }
        Box::new(GLES2Native { _gl_lifetime: PhantomData })
    }
    unsafe fn make_current_unchecked_for_window<'gl_ctx>(
        &'gl_ctx mut self,
        make_current_fn: &mut dyn FnMut(&GLContext),
        loader_fn: &mut dyn FnMut(&'static str) -> *const std::ffi::c_void,
    ) -> Box<dyn GLES + 'gl_ctx> {
        if !self.gl_ctx.is_current() || !self.is_loaded {
            make_current_fn(&self.gl_ctx);
            gles20::load_with(loader_fn);
            self.is_loaded = true;
        }
        Box::new(GLES2Native { _gl_lifetime: PhantomData })
    }
}
pub struct GLES2Native<'gl_ctx> {
    _gl_lifetime: PhantomData<&'gl_ctx ()>,
}
#[allow(non_snake_case, unused_variables)]
impl GLES for GLES2Native<'_> {
    unsafe fn driver_description(&self) -> String {
        let version = CStr::from_ptr(gles20::GetString(gles20::VERSION) as *const _);
        let vendor = CStr::from_ptr(gles20::GetString(gles20::VENDOR) as *const _);
        let renderer = CStr::from_ptr(gles20::GetString(gles20::RENDERER) as *const _);
        format!("{} / {} / {}", version.to_string_lossy(), vendor.to_string_lossy(), renderer.to_string_lossy())
    }

    unsafe fn GetError(&mut self) -> GLenum { gles20::GetError() }

    unsafe fn Enable(&mut self, cap: GLenum) { gles20::Enable(cap) }

    unsafe fn IsEnabled(&mut self, cap: GLenum) -> GLboolean { gles20::IsEnabled(cap) }

    unsafe fn Disable(&mut self, cap: GLenum) { gles20::Disable(cap) }

    unsafe fn ClientActiveTexture(&mut self, texture: GLenum) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn EnableClientState(&mut self, array: GLenum) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn DisableClientState(&mut self, array: GLenum) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn GetBooleanv(&mut self, pname: GLenum, params: *mut GLboolean) { gles20::GetBooleanv(pname, params) }

    unsafe fn GetFloatv(&mut self, pname: GLenum, params: *mut GLfloat) { gles20::GetFloatv(pname, params) }

    unsafe fn GetIntegerv(&mut self, pname: GLenum, params: *mut GLint) { gles20::GetIntegerv(pname, params) }

    unsafe fn GetTexEnviv(&mut self, target: GLenum, pname: GLenum, params: *mut GLint) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn GetTexEnvfv(&mut self, target: GLenum, pname: GLenum, params: *mut GLfloat) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn GetPointerv(&mut self, pname: GLenum, params: *mut *const GLvoid) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Hint(&mut self, target: GLenum, mode: GLenum) { gles20::Hint(target, mode) }

    unsafe fn Finish(&mut self) { gles20::Finish() }

    unsafe fn Flush(&mut self) { gles20::Flush() }

    unsafe fn GetString(&mut self, name: GLenum) -> *const GLubyte { gles20::GetString(name) }

    unsafe fn AlphaFunc(&mut self, func: GLenum, ref_: GLclampf) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn AlphaFuncx(&mut self, func: GLenum, ref_: GLclampx) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn BlendFunc(&mut self, sfactor: GLenum, dfactor: GLenum) { gles20::BlendFunc(sfactor, dfactor) }

    unsafe fn BlendEquationOES(&mut self, mode: GLenum) { gles20::BlendEquation(mode) }

    unsafe fn ColorMask(&mut self, red: GLboolean, green: GLboolean, blue: GLboolean, alpha: GLboolean,) { gles20::ColorMask(red, green, blue, alpha, ) }

    unsafe fn ClipPlanef(&mut self, plane: GLenum, equation: *const GLfloat) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn ClipPlanex(&mut self, plane: GLenum, equation: *const GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn CullFace(&mut self, mode: GLenum) { gles20::CullFace(mode) }

    unsafe fn DepthFunc(&mut self, func: GLenum) { gles20::DepthFunc(func) }

    unsafe fn DepthMask(&mut self, flag: GLboolean) { gles20::DepthMask(flag) }

    unsafe fn DepthRangef(&mut self, near: GLclampf, far: GLclampf) { gles20::DepthRangef(near, far) }

    unsafe fn DepthRangex(&mut self, near: GLclampx, far: GLclampx) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn FrontFace(&mut self, mode: GLenum) { gles20::FrontFace(mode) }

    unsafe fn PolygonOffset(&mut self, factor: GLfloat, units: GLfloat) { gles20::PolygonOffset(factor, units) }

    unsafe fn PolygonOffsetx(&mut self, factor: GLfixed, units: GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn SampleCoverage(&mut self, value: GLclampf, invert: GLboolean) { gles20::SampleCoverage(value, invert) }

    unsafe fn SampleCoveragex(&mut self, value: GLclampx, invert: GLboolean) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn ShadeModel(&mut self, mode: GLenum) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Scissor(&mut self, x: GLint, y: GLint, width: GLsizei, height: GLsizei) { gles20::Scissor(x, y, width, height) }

    unsafe fn Viewport(&mut self, x: GLint, y: GLint, width: GLsizei, height: GLsizei) { gles20::Viewport(x, y, width, height) }

    unsafe fn LineWidth(&mut self, val: GLfloat) { gles20::LineWidth(val) }

    unsafe fn LineWidthx(&mut self, val: GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn StencilFunc(&mut self, func: GLenum, ref_: GLint, mask: GLuint) { gles20::StencilFunc(func, ref_, mask) }

    unsafe fn StencilOp(&mut self, sfail: GLenum, dpfail: GLenum, dppass: GLenum) { gles20::StencilOp(sfail, dpfail, dppass) }

    unsafe fn StencilMask(&mut self, mask: GLuint) { gles20::StencilMask(mask) }

    unsafe fn LogicOp(&mut self, opcode: GLenum) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn PointSize(&mut self, size: GLfloat) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn PointSizex(&mut self, size: GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn PointParameterf(&mut self, pname: GLenum, param: GLfloat) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn PointParameterx(&mut self, pname: GLenum, param: GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn PointParameterfv(&mut self, pname: GLenum, params: *const GLfloat) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn PointParameterxv(&mut self, pname: GLenum, params: *const GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Fogf(&mut self, pname: GLenum, param: GLfloat) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Fogx(&mut self, pname: GLenum, param: GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Fogfv(&mut self, pname: GLenum, params: *const GLfloat) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Fogxv(&mut self, pname: GLenum, params: *const GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Lightf(&mut self, light: GLenum, pname: GLenum, param: GLfloat) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Lightx(&mut self, light: GLenum, pname: GLenum, param: GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Lightfv(&mut self, light: GLenum, pname: GLenum, params: *const GLfloat) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Lightxv(&mut self, light: GLenum, pname: GLenum, params: *const GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn LightModelf(&mut self, pname: GLenum, param: GLfloat) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn LightModelx(&mut self, pname: GLenum, param: GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn LightModelfv(&mut self, pname: GLenum, params: *const GLfloat) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn LightModelxv(&mut self, pname: GLenum, params: *const GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Materialf(&mut self, face: GLenum, pname: GLenum, param: GLfloat) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Materialx(&mut self, face: GLenum, pname: GLenum, param: GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Materialfv(&mut self, face: GLenum, pname: GLenum, params: *const GLfloat) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Materialxv(&mut self, face: GLenum, pname: GLenum, params: *const GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn IsBuffer(&mut self, buffer: GLuint) -> GLboolean { gles20::IsBuffer(buffer) }

    unsafe fn GenBuffers(&mut self, n: GLsizei, buffers: *mut GLuint) { gles20::GenBuffers(n, buffers) }

    unsafe fn DeleteBuffers(&mut self, n: GLsizei, buffers: *const GLuint) { gles20::DeleteBuffers(n, buffers) }

    unsafe fn BindBuffer(&mut self, target: GLenum, buffer: GLuint) { gles20::BindBuffer(target, buffer) }

    unsafe fn BufferData(&mut self, target: GLenum, size: GLsizeiptr, data: *const GLvoid, usage: GLenum,) { gles20::BufferData(target, size, data, usage, ) }

    unsafe fn BufferSubData(&mut self, target: GLenum, offset: GLintptr, size: GLsizeiptr, data: *const GLvoid,) { gles20::BufferSubData(target, offset, size, data, ) }

    unsafe fn Color4f(&mut self, red: GLfloat, green: GLfloat, blue: GLfloat, alpha: GLfloat) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Color4x(&mut self, red: GLfixed, green: GLfixed, blue: GLfixed, alpha: GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Color4ub(&mut self, red: GLubyte, green: GLubyte, blue: GLubyte, alpha: GLubyte) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Normal3f(&mut self, nx: GLfloat, ny: GLfloat, nz: GLfloat) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Normal3x(&mut self, nx: GLfixed, ny: GLfixed, nz: GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn ColorPointer(&mut self, size: GLint, type_: GLenum, stride: GLsizei, pointer: *const GLvoid,) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn NormalPointer(&mut self, type_: GLenum, stride: GLsizei, pointer: *const GLvoid) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn TexCoordPointer(&mut self, size: GLint, type_: GLenum, stride: GLsizei, pointer: *const GLvoid,) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn VertexPointer(&mut self, size: GLint, type_: GLenum, stride: GLsizei, pointer: *const GLvoid,) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn DrawArrays(&mut self, mode: GLenum, first: GLint, count: GLsizei) { gles20::DrawArrays(mode, first, count) }

    unsafe fn DrawElements(&mut self, mode: GLenum, count: GLsizei, type_: GLenum, indices: *const GLvoid,) { gles20::DrawElements(mode, count, type_, indices, ) }

    unsafe fn Clear(&mut self, mask: GLbitfield) { gles20::Clear(mask) }

    unsafe fn ClearColor(&mut self, red: GLclampf, green: GLclampf, blue: GLclampf, alpha: GLclampf,) { gles20::ClearColor(red, green, blue, alpha, ) }

    unsafe fn ClearColorx(&mut self, red: GLclampx, green: GLclampx, blue: GLclampx, alpha: GLclampx,) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn ClearDepthf(&mut self, depth: GLclampf) { gles20::ClearDepthf(depth) }

    unsafe fn ClearDepthx(&mut self, depth: GLclampx) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn ClearStencil(&mut self, s: GLint) { gles20::ClearStencil(s) }

    unsafe fn PixelStorei(&mut self, pname: GLenum, param: GLint) { gles20::PixelStorei(pname, param) }

    unsafe fn ReadPixels(&mut self, x: GLint, y: GLint, width: GLsizei, height: GLsizei, format: GLenum, type_: GLenum, pixels: *mut GLvoid,) { gles20::ReadPixels(x, y, width, height, format, type_, pixels, ) }

    unsafe fn GenTextures(&mut self, n: GLsizei, textures: *mut GLuint) { gles20::GenTextures(n, textures) }

    unsafe fn DeleteTextures(&mut self, n: GLsizei, textures: *const GLuint) { gles20::DeleteTextures(n, textures) }

    unsafe fn ActiveTexture(&mut self, texture: GLenum) { gles20::ActiveTexture(texture) }

    unsafe fn IsTexture(&mut self, texture: GLuint) -> GLboolean { gles20::IsTexture(texture) }

    unsafe fn BindTexture(&mut self, target: GLenum, texture: GLuint) { gles20::BindTexture(target, texture) }

    unsafe fn TexParameteri(&mut self, target: GLenum, pname: GLenum, param: GLint) { gles20::TexParameteri(target, pname, param) }

    unsafe fn TexParameterf(&mut self, target: GLenum, pname: GLenum, param: GLfloat) { gles20::TexParameterf(target, pname, param) }

    unsafe fn TexParameterx(&mut self, target: GLenum, pname: GLenum, param: GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn TexParameteriv(&mut self, target: GLenum, pname: GLenum, params: *const GLint) { gles20::TexParameteriv(target, pname, params) }

    unsafe fn TexParameterfv(&mut self, target: GLenum, pname: GLenum, params: *const GLfloat) { gles20::TexParameterfv(target, pname, params) }

    unsafe fn TexParameterxv(&mut self, target: GLenum, pname: GLenum, params: *const GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn TexImage2D(&mut self, target: GLenum, level: GLint, internalformat: GLint, width: GLsizei, height: GLsizei, border: GLint, format: GLenum, type_: GLenum, pixels: *const GLvoid,) { gles20::TexImage2D(target, level, internalformat, width, height, border, format, type_, pixels, ) }

    unsafe fn TexSubImage2D(&mut self, target: GLenum, level: GLint, xoffset: GLint, yoffset: GLint, width: GLsizei, height: GLsizei, format: GLenum, type_: GLenum, pixels: *const GLvoid,) { gles20::TexSubImage2D(target, level, xoffset, yoffset, width, height, format, type_, pixels, ) }

    unsafe fn CompressedTexImage2D(
        &mut self, target: GLenum, level: GLint, internalformat: GLenum,
        width: GLsizei, height: GLsizei, border: GLint, image_size: GLsizei,
        data: *const GLvoid,
    ) {
        if image_size > 0 && !data.is_null() {
            let bytes = std::slice::from_raw_parts(data.cast::<u8>(), image_size as usize);
            if super::util::try_decode_pvrtc(
                self, target, level, internalformat, width, height, border, bytes,
            ) {
                log_once!("Decompressing PVRTC textures for OpenGL ES 2.0");
                return;
            }
        }
        gles20::CompressedTexImage2D(
            target, level, internalformat, width, height, border, image_size, data,
        )
    }

    unsafe fn CopyTexImage2D(&mut self, target: GLenum, level: GLint, internalformat: GLenum, x: GLint, y: GLint, width: GLsizei, height: GLsizei, border: GLint,) { gles20::CopyTexImage2D(target, level, internalformat, x, y, width, height, border, ) }

    unsafe fn CopyTexSubImage2D(&mut self, target: GLenum, level: GLint, xoffset: GLint, yoffset: GLint, x: GLint, y: GLint, width: GLsizei, height: GLsizei,) { gles20::CopyTexSubImage2D(target, level, xoffset, yoffset, x, y, width, height, ) }

    unsafe fn TexEnvf(&mut self, target: GLenum, pname: GLenum, param: GLfloat) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn TexEnvx(&mut self, target: GLenum, pname: GLenum, param: GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn TexEnvi(&mut self, target: GLenum, pname: GLenum, param: GLint) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn TexEnvfv(&mut self, target: GLenum, pname: GLenum, params: *const GLfloat) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn TexEnvxv(&mut self, target: GLenum, pname: GLenum, params: *const GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn TexEnviv(&mut self, target: GLenum, pname: GLenum, params: *const GLint) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn MultiTexCoord4f(&mut self, target: GLenum, s: GLfloat, t: GLfloat, r: GLfloat, q: GLfloat,) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn MultiTexCoord4x(&mut self, target: GLenum, s: GLfixed, t: GLfixed, r: GLfixed, q: GLfixed,) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn MatrixMode(&mut self, mode: GLenum) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn LoadIdentity(&mut self) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn LoadMatrixf(&mut self, m: *const GLfloat) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn LoadMatrixx(&mut self, m: *const GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn MultMatrixf(&mut self, m: *const GLfloat) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn MultMatrixx(&mut self, m: *const GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn PushMatrix(&mut self) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn PopMatrix(&mut self) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Orthof(&mut self, left: GLfloat, right: GLfloat, bottom: GLfloat, top: GLfloat, near: GLfloat, far: GLfloat,) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Orthox(&mut self, left: GLfixed, right: GLfixed, bottom: GLfixed, top: GLfixed, near: GLfixed, far: GLfixed,) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Frustumf(&mut self, left: GLfloat, right: GLfloat, bottom: GLfloat, top: GLfloat, near: GLfloat, far: GLfloat,) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Frustumx(&mut self, left: GLfixed, right: GLfixed, bottom: GLfixed, top: GLfixed, near: GLfixed, far: GLfixed,) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Rotatef(&mut self, angle: GLfloat, x: GLfloat, y: GLfloat, z: GLfloat) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Rotatex(&mut self, angle: GLfixed, x: GLfixed, y: GLfixed, z: GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Scalef(&mut self, x: GLfloat, y: GLfloat, z: GLfloat) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Scalex(&mut self, x: GLfixed, y: GLfixed, z: GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Translatef(&mut self, x: GLfloat, y: GLfloat, z: GLfloat) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn Translatex(&mut self, x: GLfixed, y: GLfixed, z: GLfixed) { { log_once!("Unsupported GLES 1.x fixed-function entry point in GLES 2.0 context"); } }

    unsafe fn GenFramebuffersOES(&mut self, n: GLsizei, framebuffers: *mut GLuint) { gles20::GenFramebuffers(n, framebuffers) }

    unsafe fn GenRenderbuffersOES(&mut self, n: GLsizei, renderbuffers: *mut GLuint) { gles20::GenRenderbuffers(n, renderbuffers) }

    unsafe fn IsFramebufferOES(&mut self, framebuffer: GLuint) -> GLboolean { gles20::IsFramebuffer(framebuffer) }

    unsafe fn IsRenderbufferOES(&mut self, renderbuffer: GLuint) -> GLboolean { gles20::IsRenderbuffer(renderbuffer) }

    unsafe fn BindFramebufferOES(&mut self, target: GLenum, framebuffer: GLuint) { gles20::BindFramebuffer(target, framebuffer) }

    unsafe fn BindRenderbufferOES(&mut self, target: GLenum, renderbuffer: GLuint) { gles20::BindRenderbuffer(target, renderbuffer) }

    unsafe fn RenderbufferStorageOES(&mut self, target: GLenum, internalformat: GLenum, width: GLsizei, height: GLsizei,) { gles20::RenderbufferStorage(target, internalformat, width, height, ) }

    unsafe fn FramebufferRenderbufferOES(&mut self, target: GLenum, attachment: GLenum, renderbuffertarget: GLenum, renderbuffer: GLuint,) { gles20::FramebufferRenderbuffer(target, attachment, renderbuffertarget, renderbuffer, ) }

    unsafe fn FramebufferTexture2DOES(&mut self, target: GLenum, attachment: GLenum, textarget: GLenum, texture: GLuint, level: i32,) { gles20::FramebufferTexture2D(target, attachment, textarget, texture, level, ) }

    unsafe fn GetFramebufferAttachmentParameterivOES(&mut self, target: GLenum, attachment: GLenum, pname: GLenum, params: *mut GLint,) { gles20::GetFramebufferAttachmentParameteriv(target, attachment, pname, params, ) }

    unsafe fn GetRenderbufferParameterivOES(&mut self, target: GLenum, pname: GLenum, params: *mut GLint,) { gles20::GetRenderbufferParameteriv(target, pname, params, ) }

    unsafe fn CheckFramebufferStatusOES(&mut self, target: GLenum) -> GLenum { gles20::CheckFramebufferStatus(target) }

    unsafe fn DeleteFramebuffersOES(&mut self, n: GLsizei, framebuffers: *const GLuint) { gles20::DeleteFramebuffers(n, framebuffers) }

    unsafe fn DeleteRenderbuffersOES(&mut self, n: GLsizei, renderbuffers: *const GLuint) { gles20::DeleteRenderbuffers(n, renderbuffers) }

    unsafe fn GenerateMipmapOES(&mut self, target: GLenum) { gles20::GenerateMipmap(target) }

    unsafe fn GetBufferParameteriv(&mut self, target: GLenum, pname: GLenum, params: *mut GLint) { gles20::GetBufferParameteriv(target, pname, params) }

    unsafe fn MapBufferOES(&mut self, target: GLenum, access: GLenum) -> *mut GLvoid { std::ptr::null_mut() }

    unsafe fn UnmapBufferOES(&mut self, target: GLenum) -> GLboolean { 0 }
}

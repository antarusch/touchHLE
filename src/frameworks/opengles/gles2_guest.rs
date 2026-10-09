/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Guest OpenGL ES 2.0 shader/program, attribute and core-FBO entry points.
//! Shared ES 1.1/2.0 symbols use gles_guest and the active GLES backend.

use crate::dyld::{export_c_func, FunctionExports};
use crate::frameworks::opengles::eagl::EAGLContextHostObject;
use crate::gles::gles20_raw as gl;
use crate::gles::gles20_raw::types::*;
use crate::mem::{ConstPtr, ConstVoidPtr, GuestUSize, Mem, MutPtr};
use crate::objc::nil;
use crate::Environment;
use std::ffi::CString;

fn with_es2<T: Default>(env: &mut Environment, callback: impl FnOnce(&mut Mem) -> T) -> T {
    let context = env
        .framework_state
        .opengles
        .current_ctx_for_thread(env.current_thread);
    let Some(context) = *context else {
        return T::default();
    };
    if context == nil
        || env
            .objc
            .borrow::<EAGLContextHostObject>(context)
            .rendering_api
            != 2
    {
        log_once!("OpenGL ES 2 entry point invoked without an ES 2.0 context");
        return T::default();
    }
    let _ctx = super::sync_context(
        &mut env.framework_state.opengles,
        &mut env.objc,
        env.window.as_mut().expect("OpenGL ES needs a window"),
        env.current_thread,
    );
    callback(&mut env.mem)
}

fn glCreateShader(env: &mut Environment, shader_type: GLenum) -> GLuint {
    with_es2(env, |_| unsafe { gl::CreateShader(shader_type) })
}
fn glDeleteShader(env: &mut Environment, shader: GLuint) {
    with_es2(env, |_| unsafe { gl::DeleteShader(shader) });
}
fn glCompileShader(env: &mut Environment, shader: GLuint) {
    with_es2(env, |_| unsafe {
        gl::CompileShader(shader);
        let mut compiled: GLint = 0;
        gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut compiled);
        if compiled == 0 {
            let mut log_length: GLint = 0;
            gl::GetShaderiv(shader, gl::INFO_LOG_LENGTH, &mut log_length);
            let mut buffer = vec![0u8; log_length.max(1) as usize];
            gl::GetShaderInfoLog(
                shader,
                buffer.len() as GLsizei,
                std::ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
            );
            log!(
                "GLES2 shader {} failed compilation: {}",
                shader,
                String::from_utf8_lossy(&buffer)
            );
        } else {
            log_once!("GLES2 vertex/fragment shader compilation succeeded");
        }
    });
}
fn glIsShader(env: &mut Environment, shader: GLuint) -> GLboolean {
    with_es2(env, |_| unsafe { gl::IsShader(shader) })
}
fn glShaderSource(
    env: &mut Environment,
    shader: GLuint,
    count: GLsizei,
    strings: ConstPtr<ConstPtr<u8>>,
    lengths: ConstPtr<GLint>,
) {
    with_es2(env, |mem| {
        if count <= 0 {
            return;
        }
        let count: usize = count.try_into().unwrap();
        let mut sources: Vec<Vec<u8>> = Vec::with_capacity(count);
        for i in 0..count {
            let guest_string: ConstPtr<u8> = mem.read(strings + i as GuestUSize);
            if guest_string.is_null() {
                sources.push(Vec::new());
                continue;
            }
            let given_length = if lengths.is_null() {
                -1
            } else {
                mem.read(lengths + i as GuestUSize)
            };
            let bytes = if given_length < 0 {
                mem.cstr_at_utf8(guest_string).unwrap().as_bytes().to_vec()
            } else {
                let n = given_length as GuestUSize;
                let p = mem.ptr_at(guest_string, n);
                unsafe { std::slice::from_raw_parts(p, n as usize).to_vec() }
            };
            sources.push(bytes);
        }
        let pointers: Vec<*const GLchar> = sources
            .iter()
            .map(|s| s.as_ptr() as *const GLchar)
            .collect();
        let counts: Vec<GLint> = sources.iter().map(|s| s.len() as GLint).collect();
        log_once!("OpenGL ES 2 glShaderSource invoked");
        unsafe { gl::ShaderSource(shader, count as GLsizei, pointers.as_ptr(), counts.as_ptr()) };
    });
}
fn glGetShaderiv(env: &mut Environment, shader: GLuint, pname: GLenum, params: MutPtr<GLint>) {
    with_es2(env, |mem| unsafe {
        gl::GetShaderiv(shader, pname, mem.ptr_at_mut(params, 1))
    });
}
fn glGetShaderInfoLog(
    env: &mut Environment,
    shader: GLuint,
    max_len: GLsizei,
    length: MutPtr<GLsizei>,
    info_log: MutPtr<GLchar>,
) {
    with_es2(env, |mem| unsafe {
        let len = if length.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(length, 1)
        };
        let out = if max_len <= 0 || info_log.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(info_log, max_len as GuestUSize)
        };
        gl::GetShaderInfoLog(shader, max_len, len, out);
    });
}
fn glCreateProgram(env: &mut Environment) -> GLuint {
    with_es2(env, |_| unsafe { gl::CreateProgram() })
}
fn glDeleteProgram(env: &mut Environment, program: GLuint) {
    with_es2(env, |_| unsafe { gl::DeleteProgram(program) });
}
fn glIsProgram(env: &mut Environment, program: GLuint) -> GLboolean {
    with_es2(env, |_| unsafe { gl::IsProgram(program) })
}
fn glAttachShader(env: &mut Environment, program: GLuint, shader: GLuint) {
    with_es2(env, |_| unsafe { gl::AttachShader(program, shader) });
}
fn glDetachShader(env: &mut Environment, program: GLuint, shader: GLuint) {
    with_es2(env, |_| unsafe { gl::DetachShader(program, shader) });
}
fn glLinkProgram(env: &mut Environment, program: GLuint) {
    with_es2(env, |_| unsafe {
        gl::LinkProgram(program);
        let mut linked: GLint = 0;
        gl::GetProgramiv(program, gl::LINK_STATUS, &mut linked);
        if linked == 0 {
            let mut log_length: GLint = 0;
            gl::GetProgramiv(program, gl::INFO_LOG_LENGTH, &mut log_length);
            let mut buffer = vec![0u8; log_length.max(1) as usize];
            gl::GetProgramInfoLog(
                program,
                buffer.len() as GLsizei,
                std::ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
            );
            log!(
                "GLES2 program {} failed linking: {}",
                program,
                String::from_utf8_lossy(&buffer)
            );
        } else {
            log_once!("GLES2 shader program linking succeeded");
        }
    });
}
fn glValidateProgram(env: &mut Environment, program: GLuint) {
    with_es2(env, |_| unsafe { gl::ValidateProgram(program) });
}
fn glUseProgram(env: &mut Environment, program: GLuint) {
    log_once!("OpenGL ES 2 glUseProgram invoked");
    with_es2(env, |_| unsafe { gl::UseProgram(program) });
}
fn glGetProgramiv(env: &mut Environment, program: GLuint, pname: GLenum, params: MutPtr<GLint>) {
    with_es2(env, |mem| unsafe {
        gl::GetProgramiv(program, pname, mem.ptr_at_mut(params, 1))
    });
}
fn glGetProgramInfoLog(
    env: &mut Environment,
    program: GLuint,
    max_len: GLsizei,
    length: MutPtr<GLsizei>,
    info_log: MutPtr<GLchar>,
) {
    with_es2(env, |mem| unsafe {
        let len = if length.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(length, 1)
        };
        let out = if max_len <= 0 || info_log.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(info_log, max_len as GuestUSize)
        };
        gl::GetProgramInfoLog(program, max_len, len, out);
    });
}
fn glBindAttribLocation(env: &mut Environment, program: GLuint, index: GLuint, name: ConstPtr<u8>) {
    with_es2(env, |mem| {
        let name = CString::new(mem.cstr_at_utf8(name).unwrap().as_bytes()).unwrap();
        unsafe { gl::BindAttribLocation(program, index, name.as_ptr() as *const GLchar) };
    });
}
fn glGetAttribLocation(env: &mut Environment, program: GLuint, name: ConstPtr<u8>) -> GLint {
    with_es2(env, |mem| {
        let name = CString::new(mem.cstr_at_utf8(name).unwrap().as_bytes()).unwrap();
        unsafe { gl::GetAttribLocation(program, name.as_ptr() as *const GLchar) }
    })
}
fn glGetUniformLocation(env: &mut Environment, program: GLuint, name: ConstPtr<u8>) -> GLint {
    with_es2(env, |mem| {
        let name = CString::new(mem.cstr_at_utf8(name).unwrap().as_bytes()).unwrap();
        unsafe { gl::GetUniformLocation(program, name.as_ptr() as *const GLchar) }
    })
}
fn glEnableVertexAttribArray(env: &mut Environment, index: GLuint) {
    with_es2(env, |_| unsafe { gl::EnableVertexAttribArray(index) });
}
fn glDisableVertexAttribArray(env: &mut Environment, index: GLuint) {
    with_es2(env, |_| unsafe { gl::DisableVertexAttribArray(index) });
}
fn glVertexAttribPointer(
    env: &mut Environment,
    index: GLuint,
    size: GLint,
    type_: GLenum,
    normalized: GLboolean,
    stride: GLsizei,
    pointer: ConstVoidPtr,
) {
    with_es2(env, |mem| unsafe {
        let mut buffer_binding: GLint = 0;
        gl::GetIntegerv(gl::ARRAY_BUFFER_BINDING, &mut buffer_binding);
        let ptr = if buffer_binding != 0 {
            pointer.to_bits() as usize as *const GLvoid
        } else if pointer.is_null() {
            std::ptr::null()
        } else {
            let component_bytes = match type_ {
                gl::FLOAT => 4,
                gl::SHORT | gl::UNSIGNED_SHORT => 2,
                _ => 1,
            };
            let byte_count = (size.max(1) as usize) * component_bytes;
            mem.ptr_at(pointer.cast::<u8>(), byte_count as GuestUSize) as *const GLvoid
        };
        gl::VertexAttribPointer(index, size, type_, normalized, stride, ptr);
    });
}
fn glVertexAttrib1f(env: &mut Environment, index: GLuint, x: GLfloat) {
    with_es2(env, |_| unsafe { gl::VertexAttrib1f(index, x) });
}
fn glVertexAttrib2f(env: &mut Environment, index: GLuint, x: GLfloat, y: GLfloat) {
    with_es2(env, |_| unsafe { gl::VertexAttrib2f(index, x, y) });
}
fn glVertexAttrib3f(env: &mut Environment, index: GLuint, x: GLfloat, y: GLfloat, z: GLfloat) {
    with_es2(env, |_| unsafe { gl::VertexAttrib3f(index, x, y, z) });
}
fn glVertexAttrib4f(
    env: &mut Environment,
    index: GLuint,
    x: GLfloat,
    y: GLfloat,
    z: GLfloat,
    w: GLfloat,
) {
    with_es2(env, |_| unsafe { gl::VertexAttrib4f(index, x, y, z, w) });
}
fn glUniform1i(env: &mut Environment, location: GLint, x: GLint) {
    with_es2(env, |_| unsafe { gl::Uniform1i(location, x) });
}
fn glUniform1f(env: &mut Environment, location: GLint, x: GLfloat) {
    with_es2(env, |_| unsafe { gl::Uniform1f(location, x) });
}
fn glUniform2f(env: &mut Environment, location: GLint, x: GLfloat, y: GLfloat) {
    with_es2(env, |_| unsafe { gl::Uniform2f(location, x, y) });
}
fn glUniform3f(env: &mut Environment, location: GLint, x: GLfloat, y: GLfloat, z: GLfloat) {
    with_es2(env, |_| unsafe { gl::Uniform3f(location, x, y, z) });
}
fn glUniform4f(
    env: &mut Environment,
    location: GLint,
    x: GLfloat,
    y: GLfloat,
    z: GLfloat,
    w: GLfloat,
) {
    with_es2(env, |_| unsafe { gl::Uniform4f(location, x, y, z, w) });
}
fn glUniformMatrix2fv(
    env: &mut Environment,
    location: GLint,
    count: GLsizei,
    transpose: GLboolean,
    values: ConstPtr<GLfloat>,
) {
    with_es2(env, |mem| unsafe {
        if count <= 0 {
            return;
        }
        gl::UniformMatrix2fv(
            location,
            count,
            transpose,
            mem.ptr_at(values, count as GuestUSize * 4),
        );
    });
}
fn glUniformMatrix3fv(
    env: &mut Environment,
    location: GLint,
    count: GLsizei,
    transpose: GLboolean,
    values: ConstPtr<GLfloat>,
) {
    with_es2(env, |mem| unsafe {
        if count <= 0 {
            return;
        }
        gl::UniformMatrix3fv(
            location,
            count,
            transpose,
            mem.ptr_at(values, count as GuestUSize * 9),
        );
    });
}
fn glUniformMatrix4fv(
    env: &mut Environment,
    location: GLint,
    count: GLsizei,
    transpose: GLboolean,
    values: ConstPtr<GLfloat>,
) {
    with_es2(env, |mem| unsafe {
        if count <= 0 {
            return;
        }
        gl::UniformMatrix4fv(
            location,
            count,
            transpose,
            mem.ptr_at(values, count as GuestUSize * 16),
        );
    });
}
// ES 2.0 core framebuffer and renderbuffer API. The GLES 1.1 guest exports
// use their OES-suffixed names, while ES2 exports these unsuffixed names.
fn glBindFramebuffer(env: &mut Environment, target: GLenum, framebuffer: GLuint) {
    with_es2(env, |_| unsafe { gl::BindFramebuffer(target, framebuffer) });
}
fn glBindRenderbuffer(env: &mut Environment, target: GLenum, buffer: GLuint) {
    with_es2(env, |_| unsafe { gl::BindRenderbuffer(target, buffer) });
}
fn glCheckFramebufferStatus(env: &mut Environment, target: GLenum) -> GLenum {
    with_es2(env, |_| unsafe { gl::CheckFramebufferStatus(target) })
}
fn glGenFramebuffers(env: &mut Environment, count: GLsizei, names: MutPtr<GLuint>) {
    with_es2(env, |mem| {
        if count > 0 {
            unsafe {
                gl::GenFramebuffers(count, mem.ptr_at_mut(names, count as GuestUSize));
            }
        }
    });
}
fn glGenRenderbuffers(env: &mut Environment, count: GLsizei, names: MutPtr<GLuint>) {
    with_es2(env, |mem| {
        if count > 0 {
            unsafe {
                gl::GenRenderbuffers(count, mem.ptr_at_mut(names, count as GuestUSize));
            }
        }
    });
}
fn glDeleteFramebuffers(env: &mut Environment, count: GLsizei, names: ConstPtr<GLuint>) {
    with_es2(env, |mem| {
        if count > 0 {
            unsafe {
                gl::DeleteFramebuffers(count, mem.ptr_at(names, count as GuestUSize));
            }
        }
    });
}
fn glDeleteRenderbuffers(env: &mut Environment, count: GLsizei, names: ConstPtr<GLuint>) {
    with_es2(env, |mem| {
        if count > 0 {
            unsafe {
                gl::DeleteRenderbuffers(count, mem.ptr_at(names, count as GuestUSize));
            }
        }
    });
}
fn glIsFramebuffer(env: &mut Environment, framebuffer: GLuint) -> GLboolean {
    with_es2(env, |_| unsafe { gl::IsFramebuffer(framebuffer) })
}
fn glIsRenderbuffer(env: &mut Environment, renderbuffer: GLuint) -> GLboolean {
    with_es2(env, |_| unsafe { gl::IsRenderbuffer(renderbuffer) })
}
fn glFramebufferTexture2D(
    env: &mut Environment,
    target: GLenum,
    attachment: GLenum,
    textarget: GLenum,
    texture: GLuint,
    level: GLint,
) {
    with_es2(env, |_| unsafe {
        gl::FramebufferTexture2D(target, attachment, textarget, texture, level)
    });
}
fn glFramebufferRenderbuffer(
    env: &mut Environment,
    target: GLenum,
    attachment: GLenum,
    renderbuffertarget: GLenum,
    renderbuffer: GLuint,
) {
    with_es2(env, |_| unsafe {
        gl::FramebufferRenderbuffer(target, attachment, renderbuffertarget, renderbuffer)
    });
}
fn glRenderbufferStorage(
    env: &mut Environment,
    target: GLenum,
    internal_format: GLenum,
    width: GLsizei,
    height: GLsizei,
) {
    with_es2(env, |_| unsafe {
        gl::RenderbufferStorage(target, internal_format, width, height)
    });
}
fn glGetRenderbufferParameteriv(
    env: &mut Environment,
    target: GLenum,
    pname: GLenum,
    params: MutPtr<GLint>,
) {
    with_es2(env, |mem| unsafe {
        gl::GetRenderbufferParameteriv(target, pname, mem.ptr_at_mut(params, 1))
    });
}
fn glGenerateMipmap(env: &mut Environment, target: GLenum) {
    with_es2(env, |_| unsafe { gl::GenerateMipmap(target) });
}
fn glUniform1fv(env: &mut Environment, location: GLint, count: GLsizei, values: ConstPtr<GLfloat>) {
    with_es2(env, |mem| {
        if count > 0 {
            let len = count as GuestUSize * 1;
            unsafe {
                gl::Uniform1fv(location, count, mem.ptr_at(values, len));
            }
        }
    });
}

fn glUniform2fv(env: &mut Environment, location: GLint, count: GLsizei, values: ConstPtr<GLfloat>) {
    with_es2(env, |mem| {
        if count > 0 {
            let len = count as GuestUSize * 2;
            unsafe {
                gl::Uniform2fv(location, count, mem.ptr_at(values, len));
            }
        }
    });
}

fn glUniform3fv(env: &mut Environment, location: GLint, count: GLsizei, values: ConstPtr<GLfloat>) {
    with_es2(env, |mem| {
        if count > 0 {
            let len = count as GuestUSize * 3;
            unsafe {
                gl::Uniform3fv(location, count, mem.ptr_at(values, len));
            }
        }
    });
}

fn glUniform4fv(env: &mut Environment, location: GLint, count: GLsizei, values: ConstPtr<GLfloat>) {
    with_es2(env, |mem| {
        if count > 0 {
            let len = count as GuestUSize * 4;
            unsafe {
                gl::Uniform4fv(location, count, mem.ptr_at(values, len));
            }
        }
    });
}

fn glUniform1iv(env: &mut Environment, location: GLint, count: GLsizei, values: ConstPtr<GLint>) {
    with_es2(env, |mem| {
        if count > 0 {
            let len = count as GuestUSize * 1;
            unsafe {
                gl::Uniform1iv(location, count, mem.ptr_at(values, len));
            }
        }
    });
}

fn glUniform2iv(env: &mut Environment, location: GLint, count: GLsizei, values: ConstPtr<GLint>) {
    with_es2(env, |mem| {
        if count > 0 {
            let len = count as GuestUSize * 2;
            unsafe {
                gl::Uniform2iv(location, count, mem.ptr_at(values, len));
            }
        }
    });
}

fn glUniform3iv(env: &mut Environment, location: GLint, count: GLsizei, values: ConstPtr<GLint>) {
    with_es2(env, |mem| {
        if count > 0 {
            let len = count as GuestUSize * 3;
            unsafe {
                gl::Uniform3iv(location, count, mem.ptr_at(values, len));
            }
        }
    });
}

fn glUniform4iv(env: &mut Environment, location: GLint, count: GLsizei, values: ConstPtr<GLint>) {
    with_es2(env, |mem| {
        if count > 0 {
            let len = count as GuestUSize * 4;
            unsafe {
                gl::Uniform4iv(location, count, mem.ptr_at(values, len));
            }
        }
    });
}

fn glUniform2i(env: &mut Environment, location: GLint, v0: GLint, v1: GLint) {
    with_es2(env, |_| unsafe { gl::Uniform2i(location, v0, v1) });
}

fn glUniform3i(env: &mut Environment, location: GLint, v0: GLint, v1: GLint, v2: GLint) {
    with_es2(env, |_| unsafe { gl::Uniform3i(location, v0, v1, v2) });
}

fn glUniform4i(env: &mut Environment, location: GLint, v0: GLint, v1: GLint, v2: GLint, v3: GLint) {
    with_es2(env, |_| unsafe { gl::Uniform4i(location, v0, v1, v2, v3) });
}

fn glGetUniformfv(
    env: &mut Environment,
    program: GLuint,
    location: GLint,
    params: MutPtr<GLfloat>,
) {
    with_es2(env, |mem| unsafe {
        gl::GetUniformfv(program, location, mem.ptr_at_mut(params, 16))
    });
}
fn glGetUniformiv(env: &mut Environment, program: GLuint, location: GLint, params: MutPtr<GLint>) {
    with_es2(env, |mem| unsafe {
        gl::GetUniformiv(program, location, mem.ptr_at_mut(params, 16))
    });
}
fn glGetShaderSource(
    env: &mut Environment,
    shader: GLuint,
    max_length: GLsizei,
    length: MutPtr<GLsizei>,
    source: MutPtr<GLchar>,
) {
    with_es2(env, |mem| unsafe {
        let length = if length.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(length, 1)
        };
        let source = if max_length <= 0 || source.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(source, max_length as GuestUSize)
        };
        gl::GetShaderSource(shader, max_length, length, source);
    });
}
fn glGetAttachedShaders(
    env: &mut Environment,
    program: GLuint,
    max_count: GLsizei,
    count: MutPtr<GLsizei>,
    shaders: MutPtr<GLuint>,
) {
    with_es2(env, |mem| unsafe {
        let count = if count.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(count, 1)
        };
        let shaders = if max_count <= 0 || shaders.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(shaders, max_count as GuestUSize)
        };
        gl::GetAttachedShaders(program, max_count, count, shaders);
    });
}
fn glGetFramebufferAttachmentParameteriv(
    env: &mut Environment,
    target: GLenum,
    attachment: GLenum,
    pname: GLenum,
    params: MutPtr<GLint>,
) {
    with_es2(env, |mem| unsafe {
        gl::GetFramebufferAttachmentParameteriv(
            target,
            attachment,
            pname,
            mem.ptr_at_mut(params, 1),
        );
    });
}
fn glGetActiveUniform(
    env: &mut Environment,
    program: GLuint,
    index: GLuint,
    max_length: GLsizei,
    length: MutPtr<GLsizei>,
    size: MutPtr<GLint>,
    uniform_type: MutPtr<GLenum>,
    name: MutPtr<GLchar>,
) {
    with_es2(env, |mem| unsafe {
        let length = if length.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(length, 1)
        };
        let size = if size.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(size, 1)
        };
        let uniform_type = if uniform_type.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(uniform_type, 1)
        };
        let name = if max_length <= 0 || name.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(name, max_length as GuestUSize)
        };
        gl::GetActiveUniform(program, index, max_length, length, size, uniform_type, name);
    });
}
fn glGetActiveAttrib(
    env: &mut Environment,
    program: GLuint,
    index: GLuint,
    max_length: GLsizei,
    length: MutPtr<GLsizei>,
    size: MutPtr<GLint>,
    attrib_type: MutPtr<GLenum>,
    name: MutPtr<GLchar>,
) {
    with_es2(env, |mem| unsafe {
        let length = if length.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(length, 1)
        };
        let size = if size.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(size, 1)
        };
        let attrib_type = if attrib_type.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(attrib_type, 1)
        };
        let name = if max_length <= 0 || name.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(name, max_length as GuestUSize)
        };
        gl::GetActiveAttrib(program, index, max_length, length, size, attrib_type, name);
    });
}
fn glGetVertexAttribiv(env: &mut Environment, index: GLuint, pname: GLenum, params: MutPtr<GLint>) {
    with_es2(env, |mem| unsafe {
        gl::GetVertexAttribiv(index, pname, mem.ptr_at_mut(params, 4))
    });
}
fn glGetVertexAttribfv(
    env: &mut Environment,
    index: GLuint,
    pname: GLenum,
    params: MutPtr<GLfloat>,
) {
    with_es2(env, |mem| unsafe {
        gl::GetVertexAttribfv(index, pname, mem.ptr_at_mut(params, 4))
    });
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(glUniform1fv(_, _, _)),
    export_c_func!(glUniform2fv(_, _, _)),
    export_c_func!(glUniform3fv(_, _, _)),
    export_c_func!(glUniform4fv(_, _, _)),
    export_c_func!(glUniform1iv(_, _, _)),
    export_c_func!(glUniform2iv(_, _, _)),
    export_c_func!(glUniform3iv(_, _, _)),
    export_c_func!(glUniform4iv(_, _, _)),
    export_c_func!(glUniform2i(_, _, _)),
    export_c_func!(glUniform3i(_, _, _, _)),
    export_c_func!(glUniform4i(_, _, _, _, _)),
    export_c_func!(glGetUniformfv(_, _, _)),
    export_c_func!(glGetUniformiv(_, _, _)),
    export_c_func!(glGetShaderSource(_, _, _, _)),
    export_c_func!(glGetAttachedShaders(_, _, _, _)),
    export_c_func!(glGetFramebufferAttachmentParameteriv(_, _, _, _)),
    export_c_func!(glGetActiveUniform(_, _, _, _, _, _, _)),
    export_c_func!(glGetActiveAttrib(_, _, _, _, _, _, _)),
    export_c_func!(glGetVertexAttribiv(_, _, _)),
    export_c_func!(glGetVertexAttribfv(_, _, _)),
    export_c_func!(glCreateShader(_)),
    export_c_func!(glDeleteShader(_)),
    export_c_func!(glCompileShader(_)),
    export_c_func!(glIsShader(_)),
    export_c_func!(glShaderSource(_, _, _, _)),
    export_c_func!(glGetShaderiv(_, _, _)),
    export_c_func!(glGetShaderInfoLog(_, _, _, _)),
    export_c_func!(glCreateProgram()),
    export_c_func!(glDeleteProgram(_)),
    export_c_func!(glIsProgram(_)),
    export_c_func!(glAttachShader(_, _)),
    export_c_func!(glDetachShader(_, _)),
    export_c_func!(glLinkProgram(_)),
    export_c_func!(glValidateProgram(_)),
    export_c_func!(glUseProgram(_)),
    export_c_func!(glGetProgramiv(_, _, _)),
    export_c_func!(glGetProgramInfoLog(_, _, _, _)),
    export_c_func!(glBindAttribLocation(_, _, _)),
    export_c_func!(glGetAttribLocation(_, _)),
    export_c_func!(glGetUniformLocation(_, _)),
    export_c_func!(glEnableVertexAttribArray(_)),
    export_c_func!(glDisableVertexAttribArray(_)),
    export_c_func!(glVertexAttribPointer(_, _, _, _, _, _)),
    export_c_func!(glVertexAttrib1f(_, _)),
    export_c_func!(glVertexAttrib2f(_, _, _)),
    export_c_func!(glVertexAttrib3f(_, _, _, _)),
    export_c_func!(glVertexAttrib4f(_, _, _, _, _)),
    export_c_func!(glUniform1i(_, _)),
    export_c_func!(glUniform1f(_, _)),
    export_c_func!(glUniform2f(_, _, _)),
    export_c_func!(glUniform3f(_, _, _, _)),
    export_c_func!(glUniform4f(_, _, _, _, _)),
    export_c_func!(glUniformMatrix2fv(_, _, _, _)),
    export_c_func!(glUniformMatrix3fv(_, _, _, _)),
    export_c_func!(glUniformMatrix4fv(_, _, _, _)),
    export_c_func!(glBindFramebuffer(_, _)),
    export_c_func!(glBindRenderbuffer(_, _)),
    export_c_func!(glCheckFramebufferStatus(_)),
    export_c_func!(glGenFramebuffers(_, _)),
    export_c_func!(glGenRenderbuffers(_, _)),
    export_c_func!(glDeleteFramebuffers(_, _)),
    export_c_func!(glDeleteRenderbuffers(_, _)),
    export_c_func!(glIsFramebuffer(_)),
    export_c_func!(glIsRenderbuffer(_)),
    export_c_func!(glFramebufferTexture2D(_, _, _, _, _)),
    export_c_func!(glFramebufferRenderbuffer(_, _, _, _)),
    export_c_func!(glRenderbufferStorage(_, _, _, _)),
    export_c_func!(glGetRenderbufferParameteriv(_, _, _)),
    export_c_func!(glGenerateMipmap(_)),
];

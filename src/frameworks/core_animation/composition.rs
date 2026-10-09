/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! The implementation of layer compositing.
//!
//! This is completely original; I don't think Apple document how this works and
//! I haven't attempted to reverse-engineer the details. As such, it probably
//! diverges wildly from what the real iPhone OS does.
#![allow(clippy::zero_ptr)] // alas, as you know, opengl

use super::ca_eagl_layer::find_fullscreen_eagl_layer;
use super::ca_layer::{CALayerHostObject, DEFAULT_CONTENTS_CENTER};
use crate::frameworks::core_animation::animation;
use crate::frameworks::core_graphics::cg_color::CGColorHostObject;
use crate::frameworks::core_graphics::{cg_bitmap_context, cg_image, CGFloat, CGRect, CGSize};
use crate::gles::gles11_raw as gles11; // constants only
use crate::gles::gles11_raw::types::*;
use crate::gles::present::{present_frame, FpsCounter};
use crate::gles::GLES; // constants only
use crate::image::Image;
use crate::matrix::Matrix;
use crate::mem::SafeWrite;
use crate::objc::{id, msg, msg_class, nil, ObjC};
use crate::Environment;
use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct State {
    texture_framebuffer: Option<(GLuint, GLuint, GLuint)>,
    recomposite_next: Option<Instant>,
    fps_counter: Option<FpsCounter>,
    misc_gl_objects: Option<MiscGlObjects>,
}

struct MiscGlObjects {
    /// Texture containing a single rounded corner.
    rounded_corner_texture: GLuint,
    /// [BASIC_SQUARE_POINTS], used as both vertex and texture co-ords for
    /// drawing simple textured quads.
    basic_square_buffer: GLuint,
    /// [FLIPPED_SQUARE_POINTS], used as texture co-ords for some textured
    /// quads.
    flipped_square_buffer: GLuint,
    /// 9-patch rounded corner texture co-ords (always the same).
    rounded_vertex_buffer: GLuint,
    /// 9-patch rounded corner vertex co-ords (varies with ratio of corner
    /// radius to overall rectangle size).
    rounded_tex_coord_buffer: GLuint,
    /// Dynamic 9-patch buffers for stretchable layer contents.
    stretch_vertex_buffer: GLuint,
    stretch_tex_coord_buffer: GLuint,
    /// Index buffer for 9-patch (first 6 elements can be used for square).
    index_buffer: GLuint,
}

unsafe fn load_matrix(gles: &mut dyn GLES, matrix: Matrix<4>) {
    gles.LoadMatrixf(matrix.columns().as_ptr() as *const _);
}

static PRESENTED_LAYER_SEEN: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
static PRESENTED_FRAME_COUNT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

// Read nine points to distinguish black composition from failed presentation.
unsafe fn lit_framebuffer_samples(
    gles: &mut dyn GLES,
    left: u32,
    bottom: u32,
    width: u32,
    height: u32,
) -> usize {
    if width == 0 || height == 0 {
        return 0;
    }
    let mut lit = 0;
    for row in 0_u32..3 {
        for col in 0_u32..3 {
            let x = left + (2 * col + 1) * width / 6;
            let y = bottom + (2 * row + 1) * height / 6;
            let mut pixel = [0_u8; 4];
            gles.ReadPixels(
                x as _,
                y as _,
                1,
                1,
                gles11::RGBA,
                gles11::UNSIGNED_BYTE,
                pixel.as_mut_ptr().cast(),
            );
            if pixel[..3].iter().any(|&channel| channel > 24) {
                lit += 1;
            }
        }
    }
    lit
}

/// For use by `NSRunLoop`: call this 60 times per second. Composites the app's
/// visible layers (i.e. UI) and presents it to the screen. Does nothing if
/// composition isn't in use or it's too soon (the latter check is skipped if
/// `force` is set to [true]).
///
/// Returns the time a recomposite is due, if any.
pub fn recomposite_if_necessary(env: &mut Environment, force: bool) -> Option<Instant> {
    let mut animation_state = animation::State::default();
    let windows = env.framework_state.uikit.ui_view.ui_window.windows.clone();
    if !windows.iter().any(|&window| !msg![env; window isHidden]) {
        log_dbg!("No visible windows, skipping composition");
        return None;
    }

    if find_fullscreen_eagl_layer(env) != nil {
        // No composition done, EAGLContext will present directly.
        log_once!("Core Animation is bypassing the layer compositor via fullscreen CAEAGLLayer");
        log_dbg!("Using CAEAGLLayer fast path, skipping composition");
        return None;
    }

    if env.options.print_fps {
        env.framework_state
            .core_animation
            .composition
            .fps_counter
            .get_or_insert_with(FpsCounter::start)
            .count_frame(format_args!("Core Animation compositor"));
    }

    let now = Instant::now();
    let interval = 1.0 / 60.0; // 60Hz
    let new_recomposite_next = if let Some(recomposite_next) = env
        .framework_state
        .core_animation
        .composition
        .recomposite_next
    {
        if !force && recomposite_next > now {
            log_dbg!("Not recompositing yet, wait {:?}", recomposite_next - now);
            return Some(recomposite_next);
        }

        // See NSTimer implementation for a discussion of what this does.
        let overdue_by = now.duration_since(recomposite_next);
        log_dbg!("Recompositing, overdue by {:?}", overdue_by);
        // TODO: Use `.div_duration_f64()` once that is stabilized.
        let advance_by = (overdue_by.as_secs_f64() / interval).max(1.0).ceil();
        assert!(advance_by == (advance_by as u32) as f64);
        let advance_by = advance_by as u32;
        if advance_by > 1 {
            log_dbg!("Warning: compositor is lagging. It is overdue by {}s and has missed {} interval(s)!", overdue_by.as_secs_f64(), advance_by - 1);
        }
        let advance_by = Duration::from_secs_f64(interval)
            .checked_mul(advance_by)
            .unwrap();
        Some(recomposite_next.checked_add(advance_by).unwrap())
    } else {
        Some(now.checked_add(Duration::from_secs_f64(interval)).unwrap())
    };
    env.framework_state
        .core_animation
        .composition
        .recomposite_next = new_recomposite_next;

    let window_layers: Vec<id> = windows
        .into_iter()
        .map(|window| {
            let layer: id = msg![env; window layer];
            // Ensure layer bitmaps are up to date.
            display_layers(env, layer);
            layer
        })
        .collect();

    let screen_bounds: CGRect = {
        let screen: id = msg_class![env; UIScreen mainScreen];
        msg![env; screen bounds]
    };
    let scale_hack: u32 = env.options.scale_hack.get();
    let fb_width = screen_bounds.size.width as u32 * scale_hack;
    let fb_height = screen_bounds.size.height as u32 * scale_hack;
    let present_frame_args = (
        env.window().viewport(),
        env.window().rotation_matrix(),
        env.window().virtual_cursor_visible_at(),
    );

    // TODO: draw status bar if it's not hidden

    // Initial state for layer tree traversal (see composite_layer_recursive)
    let cumulative_transform = Matrix::<4>::identity();
    let opacity = 1.0;

    let window = env.window.as_mut().unwrap();
    let mut gles = window.make_internal_gl_ctx_current();

    // Set up GL objects needed for render-to-texture. We could draw directly
    // to the screen instead, but this way we can reuse the code for scaling and
    // rotating the screen and drawing the virtual cursor.
    let texture = if let Some((texture, framebuffer, _stencil)) = env
        .framework_state
        .core_animation
        .composition
        .texture_framebuffer
    {
        unsafe {
            gles.BindFramebufferOES(gles11::FRAMEBUFFER_OES, framebuffer);
        };
        texture
    } else {
        let mut texture = 0;
        let mut framebuffer = 0;
        unsafe {
            gles.GenTextures(1, &mut texture);
            gles.BindTexture(gles11::TEXTURE_2D, texture);
            gles.TexImage2D(
                gles11::TEXTURE_2D,
                0,
                gles11::RGBA as _,
                fb_width as _,
                fb_height as _,
                0,
                gles11::RGBA,
                gles11::UNSIGNED_BYTE,
                std::ptr::null(),
            );
            gles.TexParameteri(
                gles11::TEXTURE_2D,
                gles11::TEXTURE_MIN_FILTER,
                gles11::LINEAR as _,
            );
            gles.TexParameteri(
                gles11::TEXTURE_2D,
                gles11::TEXTURE_MAG_FILTER,
                gles11::LINEAR as _,
            );

            gles.GenFramebuffersOES(1, &mut framebuffer);
            gles.BindFramebufferOES(gles11::FRAMEBUFFER_OES, framebuffer);
            gles.FramebufferTexture2DOES(
                gles11::FRAMEBUFFER_OES,
                gles11::COLOR_ATTACHMENT0_OES,
                gles11::TEXTURE_2D,
                texture,
                0,
            );
            let mut stencil = 0;
            gles.GenRenderbuffersOES(1, &mut stencil);
            gles.BindRenderbufferOES(gles11::RENDERBUFFER_OES, stencil);
            gles.RenderbufferStorageOES(
                gles11::RENDERBUFFER_OES,
                0x8D48, /* STENCIL_INDEX8 */
                fb_width as _,
                fb_height as _,
            );
            gles.FramebufferRenderbufferOES(
                gles11::FRAMEBUFFER_OES,
                gles11::STENCIL_ATTACHMENT_OES,
                gles11::RENDERBUFFER_OES,
                stencil,
            );
            env.framework_state
                .core_animation
                .composition
                .texture_framebuffer = Some((texture, framebuffer, stencil));
            assert_eq!(gles.GetError(), 0);
            assert_eq!(
                gles.CheckFramebufferStatusOES(gles11::FRAMEBUFFER_OES),
                gles11::FRAMEBUFFER_COMPLETE_OES
            );
        }
        texture
    };

    // Set up various other GL objects that will be reused on every frame.
    let misc_gl_objects = env
        .framework_state
        .core_animation
        .composition
        .misc_gl_objects
        .get_or_insert_with(|| {
            let dimension = 512usize; // way larger than any reasonable corner
            let mut image = Image::from_pixel_vec(
                vec![255u8; dimension * dimension * 4],
                (dimension as _, dimension as _),
            );
            image.round_corners(dimension as _, /* four_corners: */ false, /* add_sheen: */ false);

            let mut rounded_corner_texture = 0;
            unsafe {
                gles.GenTextures(1, &mut rounded_corner_texture);
                gles.BindTexture(gles11::TEXTURE_2D, rounded_corner_texture);
                // GENERATE_MIPMAP must be set before the texture upload.
                gles.TexParameteri(
                    gles11::TEXTURE_2D,
                    gles11::GENERATE_MIPMAP,
                    gles11::TRUE as _,
                );
                upload_rgba8_pixels(gles.as_mut(), image.pixels(), (dimension as _, dimension as _));
                gles.TexParameteri(
                    gles11::TEXTURE_2D,
                    gles11::TEXTURE_MIN_FILTER,
                    gles11::LINEAR_MIPMAP_LINEAR as _,
                );
                gles.TexParameteri(
                    gles11::TEXTURE_2D,
                    gles11::TEXTURE_WRAP_S,
                    gles11::CLAMP_TO_EDGE as _,
                );
                gles.TexParameteri(
                    gles11::TEXTURE_2D,
                    gles11::TEXTURE_WRAP_T,
                    gles11::CLAMP_TO_EDGE as _,
                );
            }

            let [basic_square_buffer, flipped_square_buffer, rounded_vertex_buffer, rounded_tex_coord_buffer, stretch_vertex_buffer, stretch_tex_coord_buffer, index_buffer] = unsafe {
                let mut array_buffers = [0; 7];
                gles.GenBuffers(7, array_buffers.as_mut_ptr());
                array_buffers
            };
            unsafe {
                gles.BindBuffer(gles11::ARRAY_BUFFER, basic_square_buffer);
                upload_slice(gles.as_mut(), gles11::ARRAY_BUFFER, &BASIC_SQUARE_POINTS, gles11::STATIC_DRAW);
                gles.BindBuffer(gles11::ARRAY_BUFFER, flipped_square_buffer);
                upload_slice(gles.as_mut(), gles11::ARRAY_BUFFER, &FLIPPED_SQUARE_POINTS, gles11::STATIC_DRAW);
                gles.BindBuffer(gles11::ARRAY_BUFFER, rounded_vertex_buffer);
                upload_slice(gles.as_mut(), gles11::ARRAY_BUFFER, &[0f32; FLOATS_PER_9PATCH], gles11::DYNAMIC_DRAW);
                gles.BindBuffer(gles11::ARRAY_BUFFER, rounded_tex_coord_buffer);
                upload_slice(
                    gles.as_mut(),
                    gles11::ARRAY_BUFFER,
                    &make_9patch_coords([0.0, 1.0, 1.0, 0.0], [0.0, 1.0, 1.0, 0.0]),
                    gles11::STATIC_DRAW,
                );
                // Prevent accidental subsequent use.
                gles.BindBuffer(gles11::ARRAY_BUFFER, 0);

                gles.BindBuffer(gles11::ELEMENT_ARRAY_BUFFER, index_buffer);
                upload_slice(gles.as_mut(), gles11::ELEMENT_ARRAY_BUFFER, &make_9patch_indices(), gles11::STATIC_DRAW);
                // Prevent accidental subsequent use.
                gles.BindBuffer(gles11::ELEMENT_ARRAY_BUFFER, 0);
            }

            MiscGlObjects {
                rounded_corner_texture,
                basic_square_buffer,
                flipped_square_buffer,
                rounded_vertex_buffer,
                rounded_tex_coord_buffer,
                stretch_vertex_buffer,
                stretch_tex_coord_buffer,
                index_buffer,
            }
        });

    // Clear the framebuffer and set up state to prepare for rendering
    unsafe {
        gles.Viewport(0, 0, fb_width as _, fb_height as _);
        gles.ClearColor(0.0, 0.0, 0.0, 1.0);
        gles.Disable(gles11::STENCIL_TEST);
        gles.StencilMask(u32::MAX);
        gles.ClearStencil(0);
        gles.Clear(gles11::COLOR_BUFFER_BIT | gles11::STENCIL_BUFFER_BIT);
        gles.Color4f(1.0, 1.0, 1.0, 1.0);

        gles.MatrixMode(gles11::PROJECTION);
        // Scale down screen-space to normalized device co-ordinates, shift the
        // origin to be at the top-left rather than the center, and flip the
        // Y axis (OpenGL's points up, Core Animation's points down).
        // Using the projection matrix for this is more convenient than adding
        // an extra multiply to composite_layer_recursive.
        load_matrix(
            gles.as_mut(),
            Matrix::from(&Matrix::scale_2d(
                2.0 / screen_bounds.size.width,
                -2.0 / screen_bounds.size.height,
            ))
            .multiply(&Matrix::translate_3d(-1.0, 1.0, 0.0)),
        );
        gles.MatrixMode(gles11::MODELVIEW);
        gles.LoadIdentity();

        // One index buffer to rule them all
        gles.BindBuffer(gles11::ELEMENT_ARRAY_BUFFER, misc_gl_objects.index_buffer);
    }
    std::mem::drop(gles);

    // Report frames that contained a presented RGBA layer.
    PRESENTED_LAYER_SEEN.store(false, std::sync::atomic::Ordering::Relaxed);

    // Assumes the windows in the list are ordered back-to-front.
    // TODO: this may not be correct once we support windowLevel.
    for root_layer in window_layers {
        // Here's where the actual drawing happens
        unsafe {
            composite_layer_recursive(
                env,
                &mut animation_state,
                root_layer,
                cumulative_transform,
                opacity,
                0,
                None,
            );
        }
    }

    let probe_number = if PRESENTED_LAYER_SEEN.load(std::sync::atomic::Ordering::Relaxed) {
        PRESENTED_FRAME_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
    } else {
        0
    };
    let probe = matches!(probe_number, 1 | 30 | 120 | 300 | 600 | 1200);

    // Re-borrow
    let window = env.window.as_mut().unwrap();
    let mut gles = window.make_internal_gl_ctx_current();

    // Clean up some GL state
    unsafe {
        gles.Viewport(0, 0, fb_width as _, fb_height as _);
        gles.Color4f(1.0, 1.0, 1.0, 1.0);
        gles.Disable(gles11::BLEND);
        gles.MatrixMode(gles11::PROJECTION);
        gles.LoadIdentity();
        gles.MatrixMode(gles11::MODELVIEW);
        gles.LoadIdentity();
        gles.BindBuffer(gles11::ARRAY_BUFFER, 0);
        gles.BindBuffer(gles11::ELEMENT_ARRAY_BUFFER, 0);
        assert_eq!(gles.GetError(), 0);
    }

    let composed_lit = if probe {
        unsafe { lit_framebuffer_samples(gles.as_mut(), 0, 0, fb_width, fb_height) }
    } else {
        0
    };

    // Present our rendered frame (bound to TEXTURE_2D). This copies it to the
    // default framebuffer (0) so we need to unbind our internal framebuffer.
    unsafe {
        gles.BindTexture(gles11::TEXTURE_2D, texture);
        gles.BindFramebufferOES(gles11::FRAMEBUFFER_OES, 0);
        present_frame(
            gles.as_mut(),
            present_frame_args.0,
            present_frame_args.1,
            present_frame_args.2,
        );
        if probe {
            let (x, y, width, height) = present_frame_args.0;
            let displayed_lit = lit_framebuffer_samples(gles.as_mut(), x, y, width, height);
            log!(
                "Core Animation framebuffer probe {probe_number}: composed={composed_lit}/9 lit, presented={displayed_lit}/9 lit, source={fb_width}x{fb_height}, output={width}x{height}",
            );
        }
    }
    std::mem::drop(gles);
    window.swap_window();
    log_once!("Core Animation composited window swap completed");

    animation_state.update_started_and_finished_animations(env);

    new_recomposite_next
}

/// Call `displayIfNeeded` on all relevant layers in the tree, so their bitmaps
/// are up to date before compositing.
fn display_layers(env: &mut Environment, root_layer: id) {
    // Tell layers to redraw themselves if needed.

    fn traverse(objc: &ObjC, layer: id, layers_needing_display: &mut Vec<id>) {
        let host_obj = objc.borrow::<CALayerHostObject>(layer);
        if host_obj.hidden {
            return;
        }
        if host_obj.needs_display {
            layers_needing_display.push(layer);
        }
        for &layer in &host_obj.sublayers {
            traverse(objc, layer, layers_needing_display);
        }
    }

    let mut layers_needing_display = Vec::new();
    traverse(&env.objc, root_layer, &mut layers_needing_display);

    for layer in layers_needing_display {
        () = msg![env; layer displayIfNeeded];
    }
}

/// Traverses the layer tree and draws each layer.
unsafe fn composite_layer_recursive(
    env: &mut Environment,
    animation_state: &mut animation::State,
    layer: id,
    cumulative_transform: Matrix<4>,
    opacity: CGFloat,
    stencil_depth: u32,
    presentation: Option<CALayerHostObject>,
) {
    // TODO: back-to-front drawing is not efficient, could we use front-to-back?

    // This is both acting as the presentationLayer and the private render layer
    // It might need to be reworked in the future into a guest presentationLayer
    let host_obj =
        presentation.unwrap_or_else(|| animation_state.create_presentation_layer(env, layer));

    if host_obj.hidden {
        return;
    }

    let quad_buffer = env
        .framework_state
        .core_animation
        .composition
        .misc_gl_objects
        .as_ref()
        .unwrap()
        .basic_square_buffer;
    let window = env.window.as_mut().unwrap();
    let mut gles = window.make_internal_gl_ctx_current();

    let opacity = opacity * host_obj.opacity;
    let cumulative_transform = {
        let CALayerHostObject { bounds, .. } = host_obj;

        // Update the transform to match this layer's co-ordinate space.
        let cumulative_transform = host_obj.render_transform().multiply(&cumulative_transform);

        // Reposition and scale the unit quad (see ARRAY_BUFFER binding)
        // so it will have the right size in this layer's co-ordinate space.
        gles.MatrixMode(gles11::MODELVIEW);
        load_matrix(
            gles.as_mut(),
            Matrix::<4>::from(&Matrix::scale_2d(bounds.size.width, bounds.size.height))
                .multiply(&Matrix::translate_3d(bounds.origin.x, bounds.origin.y, 0.0))
                .multiply(&cumulative_transform),
        );

        cumulative_transform
    };

    let clipped = host_obj.masks_to_bounds;
    let depth = stencil_depth + u32::from(clipped);
    if clipped {
        change_stencil(
            quad_buffer,
            gles.as_mut(),
            cumulative_transform,
            host_obj.bounds,
            stencil_depth,
            true,
        );
    }
    if depth > 0 {
        gles.Enable(gles11::STENCIL_TEST);
        gles.StencilFunc(gles11::EQUAL, depth as _, u32::MAX);
        gles.StencilOp(gles11::KEEP, gles11::KEEP, gles11::KEEP);
    } else {
        gles.Disable(gles11::STENCIL_TEST);
    }

    // Draw background color, if any
    let have_background = if let Some(background_color) = host_obj.background_color {
        let misc = env
            .framework_state
            .core_animation
            .composition
            .misc_gl_objects
            .as_ref()
            .unwrap();

        let CGColorHostObject { r, g, b, a, .. } = background_color;
        gles.Color4f(
            r * a * opacity,
            g * a * opacity,
            b * a * opacity,
            a * opacity,
        );
        gles.Enable(gles11::BLEND);
        gles.BlendFunc(gles11::ONE, gles11::ONE_MINUS_SRC_ALPHA);

        let radius = host_obj.corner_radius;
        if radius == 0.0 {
            gles.Disable(gles11::TEXTURE_2D);
            gles.DisableClientState(gles11::TEXTURE_COORD_ARRAY);

            gles.EnableClientState(gles11::VERTEX_ARRAY);
            gles.BindBuffer(gles11::ARRAY_BUFFER, misc.basic_square_buffer);
            gles.VertexPointer(2, gles11::FLOAT, 0, 0 as *const GLvoid);

            gles.DrawElements(
                gles11::TRIANGLES,
                SQUARE_INDICES.len() as _,
                gles11::UNSIGNED_BYTE,
                0 as *const GLvoid,
            );
        } else {
            gles.Enable(gles11::TEXTURE_2D);
            gles.BindTexture(gles11::TEXTURE_2D, misc.rounded_corner_texture);
            gles.EnableClientState(gles11::TEXTURE_COORD_ARRAY);
            gles.BindBuffer(gles11::ARRAY_BUFFER, misc.rounded_tex_coord_buffer);
            gles.TexCoordPointer(2, gles11::FLOAT, 0, 0 as *const GLvoid);

            gles.EnableClientState(gles11::VERTEX_ARRAY);
            gles.BindBuffer(gles11::ARRAY_BUFFER, misc.rounded_vertex_buffer);
            upload_slice(
                gles.as_mut(),
                gles11::ARRAY_BUFFER,
                &make_9patch_coords(
                    [
                        0.0,
                        (radius / host_obj.bounds.size.width).min(0.5),
                        (1.0 - radius / host_obj.bounds.size.width).max(0.5),
                        1.0,
                    ],
                    [
                        0.0,
                        (radius / host_obj.bounds.size.height).min(0.5),
                        (1.0 - radius / host_obj.bounds.size.height).max(0.5),
                        1.0,
                    ],
                ),
                gles11::DYNAMIC_DRAW,
            );
            gles.VertexPointer(2, gles11::FLOAT, 0, 0 as *const GLvoid);

            gles.DrawElements(
                gles11::TRIANGLES,
                INDICES_PER_9PATCH as _,
                gles11::UNSIGNED_BYTE,
                0 as *const GLvoid,
            );
        };

        true
    } else {
        false
    };

    let need_texture = host_obj.presented_pixels.is_some()
        || host_obj.contents != nil
        || host_obj.cg_context.is_some();
    let need_update = need_texture && !host_obj.gles_texture_is_up_to_date;

    if need_texture {
        if let Some(texture) = host_obj.gles_texture {
            gles.BindTexture(gles11::TEXTURE_2D, texture);
        } else {
            assert!(!host_obj.gles_texture_is_up_to_date);
            let mut texture = 0;
            gles.GenTextures(1, &mut texture);
            gles.BindTexture(gles11::TEXTURE_2D, texture);
            // Update original layer texture
            env.objc.borrow_mut::<CALayerHostObject>(layer).gles_texture = Some(texture);
        }
    }

    // Update original layer texture with CAEAGLLayer pixels (slow path), if any
    if need_update {
        let original_host_obj = env.objc.borrow_mut::<CALayerHostObject>(layer);
        if let Some((ref mut pixels, width, height)) = original_host_obj.presented_pixels {
            // The pixels are always RGBA, but if the layer is opaque then the
            // alpha channel is meant to be ignored. glTexImage2D() has no
            // option to ignore it, so let's manually set them to 255.
            if original_host_obj.opaque {
                let mut i = 3;
                while i < pixels.len() {
                    pixels[i] = 255;
                    i += 4;
                }
            }

            upload_rgba8_pixels(gles.as_mut(), pixels, (width, height));
        }
    }

    // Movie/EAGL frames are the current layer contents. A stale CGImage
    // or bitmap backing must not overwrite a newer presented RGBA frame.
    if need_update && host_obj.presented_pixels.is_none() {
        if host_obj.contents != nil {
            let image = cg_image::borrow_image(&env.objc, host_obj.contents);

            // No special handling for opacity is needed here: the alpha channel
            // on an image is meaningful and won't be ignored.
            upload_rgba8_pixels(gles.as_mut(), image.pixels(), image.dimensions());
        } else if let Some(cg_context) = host_obj.cg_context {
            // Make sure this is in sync with the code in ca_layer.rs that
            // sets up the context!
            let (width, height, data) = cg_bitmap_context::get_data(&env.objc, cg_context);
            let size = width * height * 4;
            let pixels = env.mem.bytes_at(data.cast(), size);
            upload_rgba8_pixels(gles.as_mut(), pixels, (width, height));
        }
    }

    if need_update {
        // Update original layer field
        env.objc
            .borrow_mut::<CALayerHostObject>(layer)
            .gles_texture_is_up_to_date = true;
    }

    // Confirm that a submitted RGBA frame actually reaches the compositor.
    if host_obj.presented_pixels.is_some() {
        PRESENTED_LAYER_SEEN.store(true, std::sync::atomic::Ordering::Relaxed);
        static PRESENTED_LAYER_COMPOSITES: std::sync::atomic::AtomicUsize =
            std::sync::atomic::AtomicUsize::new(0);
        let number =
            PRESENTED_LAYER_COMPOSITES.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        if matches!(number, 1 | 30 | 120) {
            log!(
                "Core Animation compositing presented RGBA layer {number}: layer={layer:?}, bounds={:?}, opacity={opacity:.2}, texture_updated={need_update}",
                host_obj.bounds,
            );
        }
    }
    // Draw texture, if any
    if need_texture {
        if host_obj.contents_gravity != "resize" && host_obj.contents != nil {
            let (width, height) = cg_image::borrow_image(&env.objc, host_obj.contents).dimensions();
            let rect = gravity_rect(&host_obj.contents_gravity, (width, height), host_obj.bounds);
            load_matrix(
                gles.as_mut(),
                Matrix::<4>::from(&Matrix::scale_2d(rect.size.width, rect.size.height))
                    .multiply(&Matrix::translate_3d(rect.origin.x, rect.origin.y, 0.0))
                    .multiply(&cumulative_transform),
            );
        }
        let stretch_mesh = if host_obj.contents_center != DEFAULT_CONTENTS_CENTER {
            let dimensions = if host_obj.contents != nil {
                cg_image::borrow_image(&env.objc, host_obj.contents).dimensions()
            } else if let Some((_, width, height)) = &host_obj.presented_pixels {
                (*width, *height)
            } else {
                let (width, height, _) =
                    cg_bitmap_context::get_data(&env.objc, host_obj.cg_context.unwrap());
                (width, height)
            };
            content_stretch_mesh(
                host_obj.contents_center,
                dimensions,
                host_obj.bounds.size,
                host_obj.contents == nil,
            )
        } else {
            None
        };
        let misc = env
            .framework_state
            .core_animation
            .composition
            .misc_gl_objects
            .as_ref()
            .unwrap();

        gles.Color4f(opacity, opacity, opacity, opacity);
        if opacity == 1.0 && host_obj.opaque && !have_background {
            gles.Disable(gles11::BLEND);
        } else {
            gles.Enable(gles11::BLEND);
            gles.BlendFunc(gles11::ONE, gles11::ONE_MINUS_SRC_ALPHA);
        }

        gles.EnableClientState(gles11::VERTEX_ARRAY);
        if let Some((vertices, _)) = &stretch_mesh {
            gles.BindBuffer(gles11::ARRAY_BUFFER, misc.stretch_vertex_buffer);
            upload_slice(
                gles.as_mut(),
                gles11::ARRAY_BUFFER,
                vertices,
                gles11::DYNAMIC_DRAW,
            );
        } else {
            gles.BindBuffer(gles11::ARRAY_BUFFER, misc.basic_square_buffer);
        }
        gles.VertexPointer(2, gles11::FLOAT, 0, 0 as *const GLvoid);

        gles.EnableClientState(gles11::TEXTURE_COORD_ARRAY);
        // Normal images will have top-to-bottom row order, but OpenGL ES
        // expects bottom-to-top, so flip the UVs in that case.
        if let Some((_, tex_coords)) = &stretch_mesh {
            gles.BindBuffer(gles11::ARRAY_BUFFER, misc.stretch_tex_coord_buffer);
            upload_slice(
                gles.as_mut(),
                gles11::ARRAY_BUFFER,
                tex_coords,
                gles11::DYNAMIC_DRAW,
            );
            gles.TexParameteri(
                gles11::TEXTURE_2D,
                gles11::TEXTURE_WRAP_S,
                gles11::CLAMP_TO_EDGE as _,
            );
            gles.TexParameteri(
                gles11::TEXTURE_2D,
                gles11::TEXTURE_WRAP_T,
                gles11::CLAMP_TO_EDGE as _,
            );
        } else {
            gles.BindBuffer(
                gles11::ARRAY_BUFFER,
                if host_obj.contents != nil {
                    misc.basic_square_buffer
                } else {
                    misc.flipped_square_buffer
                },
            );
        }
        gles.TexCoordPointer(2, gles11::FLOAT, 0, 0 as *const GLvoid);
        gles.Enable(gles11::TEXTURE_2D);
        gles.DrawElements(
            gles11::TRIANGLES,
            if stretch_mesh.is_some() {
                INDICES_PER_9PATCH
            } else {
                SQUARE_INDICES.len()
            } as _,
            gles11::UNSIGNED_BYTE,
            0 as *const GLvoid,
        );
    }
    if let Some(color) = host_obj.border_color {
        let width = host_obj
            .border_width
            .min(host_obj.bounds.size.width * 0.5)
            .min(host_obj.bounds.size.height * 0.5);
        if width > 0.0 {
            let b = host_obj.bounds;
            let rects = [
                CGRect {
                    origin: b.origin,
                    size: CGSize {
                        width: b.size.width,
                        height: width,
                    },
                },
                CGRect {
                    origin: crate::frameworks::core_graphics::CGPoint {
                        x: b.origin.x,
                        y: b.origin.y + b.size.height - width,
                    },
                    size: CGSize {
                        width: b.size.width,
                        height: width,
                    },
                },
                CGRect {
                    origin: crate::frameworks::core_graphics::CGPoint {
                        x: b.origin.x,
                        y: b.origin.y + width,
                    },
                    size: CGSize {
                        width,
                        height: b.size.height - 2.0 * width,
                    },
                },
                CGRect {
                    origin: crate::frameworks::core_graphics::CGPoint {
                        x: b.origin.x + b.size.width - width,
                        y: b.origin.y + width,
                    },
                    size: CGSize {
                        width,
                        height: b.size.height - 2.0 * width,
                    },
                },
            ];
            gles.Disable(gles11::TEXTURE_2D);
            gles.DisableClientState(gles11::TEXTURE_COORD_ARRAY);
            gles.Enable(gles11::BLEND);
            gles.BlendFunc(gles11::ONE, gles11::ONE_MINUS_SRC_ALPHA);
            gles.Color4f(
                color.r * color.a * opacity,
                color.g * color.a * opacity,
                color.b * color.a * opacity,
                color.a * opacity,
            );
            for rect in rects {
                draw_quad(quad_buffer, gles.as_mut(), cumulative_transform, rect);
            }
        }
    }
    std::mem::drop(gles);

    let mut children: Vec<_> = host_obj
        .sublayers
        .iter()
        .map(|&child| (child, animation_state.create_presentation_layer(env, child)))
        .collect();
    // Stable sorting preserves insertion order at equal presentation depth.
    children.sort_by(|(_, a), (_, b)| a.z_position.total_cmp(&b.z_position));
    for (child, presentation) in children {
        composite_layer_recursive(
            env,
            animation_state,
            child,
            cumulative_transform,
            opacity,
            depth,
            Some(presentation),
        );
    }
    if clipped {
        let mut gles = env.window.as_mut().unwrap().make_internal_gl_ctx_current();
        change_stencil(
            quad_buffer,
            gles.as_mut(),
            cumulative_transform,
            host_obj.bounds,
            depth,
            false,
        );
    }
}

unsafe fn draw_quad(quad_buffer: GLuint, gles: &mut dyn GLES, transform: Matrix<4>, rect: CGRect) {
    load_matrix(
        gles,
        Matrix::<4>::from(&Matrix::scale_2d(rect.size.width, rect.size.height))
            .multiply(&Matrix::translate_3d(rect.origin.x, rect.origin.y, 0.0))
            .multiply(&transform),
    );
    gles.EnableClientState(gles11::VERTEX_ARRAY);
    gles.BindBuffer(gles11::ARRAY_BUFFER, quad_buffer);
    gles.VertexPointer(2, gles11::FLOAT, 0, std::ptr::null());
    gles.DrawElements(
        gles11::TRIANGLES,
        SQUARE_INDICES.len() as _,
        gles11::UNSIGNED_BYTE,
        std::ptr::null(),
    );
}

unsafe fn change_stencil(
    quad_buffer: GLuint,
    gles: &mut dyn GLES,
    transform: Matrix<4>,
    rect: CGRect,
    depth: u32,
    increment: bool,
) {
    gles.Enable(gles11::STENCIL_TEST);
    gles.StencilMask(u32::MAX);
    gles.StencilFunc(gles11::EQUAL, depth as _, u32::MAX);
    gles.StencilOp(
        gles11::KEEP,
        gles11::KEEP,
        if increment {
            gles11::INCR
        } else {
            gles11::DECR
        },
    );
    gles.ColorMask(0, 0, 0, 0);
    gles.Disable(gles11::TEXTURE_2D);
    gles.DisableClientState(gles11::TEXTURE_COORD_ARRAY);
    draw_quad(quad_buffer, gles, transform, rect);
    gles.ColorMask(1, 1, 1, 1);
    gles.StencilOp(gles11::KEEP, gles11::KEEP, gles11::KEEP);
    if depth == 1 && !increment {
        gles.Disable(gles11::STENCIL_TEST);
    } else {
        gles.StencilFunc(
            gles11::EQUAL,
            (if increment { depth + 1 } else { depth - 1 }) as _,
            u32::MAX,
        );
    }
}

fn gravity_rect(gravity: &str, source: (u32, u32), bounds: CGRect) -> CGRect {
    let (mut width, mut height) = (source.0 as f32, source.1 as f32);
    if width == 0.0 || height == 0.0 {
        return bounds;
    }
    if gravity == "resizeAspect" || gravity == "resizeAspectFill" {
        let x = bounds.size.width / width;
        let y = bounds.size.height / height;
        let scale = if gravity == "resizeAspect" {
            x.min(y)
        } else {
            x.max(y)
        };
        width *= scale;
        height *= scale;
    } else if gravity == "resize" {
        return bounds;
    }
    let x = if gravity.contains("Left") || gravity == "left" {
        0.0
    } else if gravity.contains("Right") || gravity == "right" {
        bounds.size.width - width
    } else {
        (bounds.size.width - width) * 0.5
    };
    let y = if gravity.starts_with("top") {
        0.0
    } else if gravity.starts_with("bottom") {
        bounds.size.height - height
    } else {
        (bounds.size.height - height) * 0.5
    };
    CGRect {
        origin: crate::frameworks::core_graphics::CGPoint {
            x: bounds.origin.x + x,
            y: bounds.origin.y + y,
        },
        size: CGSize { width, height },
    }
}

const FLOATS_PER_POINT: usize = 2;
const BASIC_SQUARE_POINTS: [f32; 4 * FLOATS_PER_POINT] = [0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0];
const SQUARE_INDICES: [u8; 6] = [0, 1, 2, 2, 1, 3];
const FLIPPED_SQUARE_POINTS: [f32; 4 * FLOATS_PER_POINT] = [0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0];
const FLOATS_PER_9PATCH: usize = BASIC_SQUARE_POINTS.len() * 3 * 3;
const INDICES_PER_9PATCH: usize = SQUARE_INDICES.len() * 3 * 3;

/// Destination edges in unit coordinates, with caps measured in source pixels.
fn stretch_axis(start: f32, length: f32, source_size: f32, target_size: f32) -> [f32; 4] {
    let leading = start * source_size;
    let trailing = (1.0 - start - length).max(0.0) * source_size;
    let caps = leading + trailing;
    // If the view is smaller than both caps combined, shrink them together
    // rather than letting them overlap or reversing the center region.
    let cap_scale = if caps > target_size {
        target_size / caps
    } else {
        1.0
    };
    [
        0.0,
        leading * cap_scale / target_size,
        1.0 - trailing * cap_scale / target_size,
        1.0,
    ]
}

fn content_stretch_mesh(
    center: CGRect,
    source_size: (u32, u32),
    target_size: CGSize,
    flip_y: bool,
) -> Option<([f32; FLOATS_PER_9PATCH], [f32; FLOATS_PER_9PATCH])> {
    let x = center.origin.x;
    let y = center.origin.y;
    let width = center.size.width;
    let height = center.size.height;
    if center == DEFAULT_CONTENTS_CENTER
        || ![x, y, width, height, target_size.width, target_size.height]
            .iter()
            .all(|v| v.is_finite())
        || x < 0.0
        || y < 0.0
        || width < 0.0
        || height < 0.0
        || x + width > 1.0
        || y + height > 1.0
        || source_size.0 == 0
        || source_size.1 == 0
        || target_size.width <= 0.0
        || target_size.height <= 0.0
    {
        return None;
    }
    let vertices = make_9patch_coords(
        stretch_axis(x, width, source_size.0 as f32, target_size.width),
        stretch_axis(y, height, source_size.1 as f32, target_size.height),
    );
    let mut tex_y = [0.0, y, y + height, 1.0];
    if flip_y {
        for value in &mut tex_y {
            *value = 1.0 - *value;
        }
    }
    Some((
        vertices,
        make_9patch_coords([0.0, x, x + width, 1.0], tex_y),
    ))
}

fn make_9patch_coords(x_edges: [f32; 4], y_edges: [f32; 4]) -> [f32; FLOATS_PER_9PATCH] {
    let mut out_points = [0.0; FLOATS_PER_9PATCH];
    #[allow(clippy::chunks_exact_to_as_chunks)]
    for (i, out_points_chunk) in out_points
        .chunks_exact_mut(BASIC_SQUARE_POINTS.len())
        .enumerate()
    {
        let (x, y) = (i % 3, i / 3);

        for (dst_xy, src_xy) in out_points_chunk
            .chunks_exact_mut(2)
            .zip(BASIC_SQUARE_POINTS.chunks_exact(2))
        {
            let (x1, x2) = (x_edges[x], x_edges[x + 1]);
            let (y1, y2) = (y_edges[y], y_edges[y + 1]);
            dst_xy[0] = x1 + src_xy[0] * (x2 - x1);
            dst_xy[1] = y1 + src_xy[1] * (y2 - y1);
        }
    }
    out_points
}

fn make_9patch_indices() -> [u8; INDICES_PER_9PATCH] {
    let mut out_indices = [0; SQUARE_INDICES.len() * 3 * 3];
    #[allow(clippy::chunks_exact_to_as_chunks)]
    for (i, out_indices_chunk) in out_indices
        .chunks_exact_mut(SQUARE_INDICES.len())
        .enumerate()
    {
        for (out_index, in_index) in out_indices_chunk
            .iter_mut()
            .zip(SQUARE_INDICES.iter().copied())
        {
            *out_index = in_index + i as u8 * (BASIC_SQUARE_POINTS.len() / FLOATS_PER_POINT) as u8;
        }
    }
    out_indices
}

unsafe fn upload_slice<T: SafeWrite>(
    gles: &mut dyn GLES,
    target: GLenum,
    data: &[T],
    usage: GLenum,
) {
    gles.BufferData(
        target,
        std::mem::size_of_val(data) as _,
        data.as_ptr() as *const _,
        usage,
    )
}

unsafe fn upload_rgba8_pixels(gles: &mut dyn GLES, pixels: &[u8], dimensions: (u32, u32)) {
    gles.TexImage2D(
        gles11::TEXTURE_2D,
        0,
        gles11::RGBA as _,
        dimensions.0 as _,
        dimensions.1 as _,
        0,
        gles11::RGBA,
        gles11::UNSIGNED_BYTE,
        pixels.as_ptr() as *const _,
    );
    gles.TexParameteri(
        gles11::TEXTURE_2D,
        gles11::TEXTURE_MIN_FILTER,
        gles11::LINEAR as _,
    );
    gles.TexParameteri(
        gles11::TEXTURE_2D,
        gles11::TEXTURE_MAG_FILTER,
        gles11::LINEAR as _,
    );
}

#[cfg(test)]
mod tests {
    use super::{content_stretch_mesh, stretch_axis, DEFAULT_CONTENTS_CENTER};
    use crate::frameworks::core_graphics::{CGPoint, CGRect, CGSize};

    fn rect(x: f32, y: f32, width: f32, height: f32) -> CGRect {
        CGRect {
            origin: CGPoint { x, y },
            size: CGSize { width, height },
        }
    }

    fn close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.00001,
            "{actual} != {expected}"
        );
    }

    #[test]
    fn stretch_preserves_asymmetric_caps() {
        let edges = stretch_axis(0.1, 0.6, 100.0, 200.0);
        close(edges[1] * 200.0, 10.0);
        close((1.0 - edges[2]) * 200.0, 30.0);
        let (vertices, uv) = content_stretch_mesh(
            rect(0.25, 0.25, 0.5, 0.5),
            (100, 80),
            CGSize {
                width: 200.0,
                height: 160.0,
            },
            false,
        )
        .unwrap();
        // A 25-pixel corner remains 25 pixels, while the middle expands to 150.
        close((vertices[4] - vertices[0]) * 200.0, 25.0);
        close((vertices[36] - vertices[32]) * 200.0, 150.0);
        close((uv[4] - uv[0]) * 100.0, 25.0);
        close((uv[36] - uv[32]) * 100.0, 50.0);
    }

    #[test]
    fn stretch_compresses_caps_without_overlap() {
        assert_eq!(stretch_axis(0.25, 0.5, 100.0, 25.0), [0.0, 0.5, 0.5, 1.0]);
        assert_eq!(stretch_axis(0.5, 0.0, 20.0, 40.0), [0.0, 0.25, 0.75, 1.0]);
    }

    #[test]
    fn stretch_mesh_zero_strip_and_orientation() {
        let size = CGSize {
            width: 80.0,
            height: 40.0,
        };
        let (vertices, uv) =
            content_stretch_mesh(rect(0.0, 0.5, 1.0, 0.0), (8, 8), size, false).unwrap();
        close((vertices[33] - vertices[35]) * 40.0, 32.0);
        close(uv[33], 0.5);
        close(uv[35], 0.5);
        let (_, uv) = content_stretch_mesh(rect(0.0, 0.25, 1.0, 0.5), (8, 8), size, true).unwrap();
        close(uv[1], 0.75);
        close(uv[3], 1.0);
    }

    #[test]
    fn stretch_mesh_default_and_invalid_sizes() {
        let size = CGSize {
            width: 80.0,
            height: 40.0,
        };
        assert!(content_stretch_mesh(DEFAULT_CONTENTS_CENTER, (8, 8), size, false).is_none());
        let center = rect(0.25, 0.25, 0.5, 0.5);
        assert!(content_stretch_mesh(center, (0, 8), size, false).is_none());
        assert!(content_stretch_mesh(center, (8, 8), CGSize::default(), false).is_none());
        assert!(content_stretch_mesh(rect(f32::NAN, 0.0, 0.5, 0.5), (8, 8), size, false).is_none());
    }
}

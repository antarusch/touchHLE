/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `MPMoviePlayerController` etc.

#[cfg(target_os = "android")]
use super::android_video::{Frame, MovieDecoder};
use crate::dyld::{ConstantExports, HostConstant};
#[cfg(target_os = "android")]
use crate::frameworks::core_animation::ca_layer::present_movie_pixels;
use crate::frameworks::core_graphics::{CGFloat, CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::{ns_string, ns_url, NSInteger, NSTimeInterval};
use crate::frameworks::uikit::ui_device::UIDeviceOrientation;
use crate::objc::{
    id, msg, msg_class, msg_super, nil, objc_classes, release, retain, todo_objc_setter,
    ClassExports, HostObject, NSZonePtr,
};
use crate::Environment;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct State {
    active_player: Option<id>,
    // Hunters 2's IntroVideoController still uses the finished player
    // during the asynchronous CoreView unload/finish handoff. Keep the
    // runtime playback retain until a replacement scene is displayed.
    #[cfg(target_os = "android")]
    hunters_completed_player_holds: Vec<id>,
    #[cfg(target_os = "android")]
    hunters_replacement_scene_displayed: bool,
    // SaveMenuController can remain attached in status 3 after a resume
    // selection, leaving the next controller permanently blocked.
    // Defer any recovery callback until outside objc_msgSend.
    #[cfg(target_os = "android")]
    hunters_pending_save_unload: Option<(id, id, Instant)>,
    // A saved game can get past SaveMenuController but remain waiting for
    // ShipGameController.onLoad. Apply a separate lifecycle-scoped recovery.
    #[cfg(target_os = "android")]
    hunters_pending_ship_load: Option<(id, id, Instant)>,
    #[cfg(target_os = "android")]
    hunters_create_save_visible: bool,
    /// Once save creation has switched to gameplay, do not draw the intro movie
    /// over the game's Core Animation / GL content.
    #[cfg(target_os = "android")]
    hunters_gameplay_transition: bool,
    #[cfg(target_os = "android")]
    hunters_create_save_controller: Option<id>,
    #[cfg(target_os = "android")]
    hunters_raised_create_save_layer: Option<(id, CGFloat)>,
    #[cfg(target_os = "android")]
    hunters_message_visible: bool,
    #[cfg(target_os = "android")]
    hunters_message_controller: Option<id>,
    #[cfg(target_os = "android")]
    hunters_raised_message_layer: Option<(id, CGFloat)>,
    #[cfg(target_os = "android")]
    hunters_resume_contract_visible: bool,
    #[cfg(target_os = "android")]
    hunters_resume_contract_controller: Option<id>,
    #[cfg(target_os = "android")]
    hunters_raised_resume_contract_layer: Option<(id, CGFloat)>,
    /// Various apps (e.g. Crash Bandicoot Nitro Kart 3D and Spore Origins)
    /// create or start a player and await some kind of notification, but can't
    /// handle it if that notification happens immediately. This queue lets us
    /// delay such notifications until the app next returns to the run loop,
    /// which seems to be late enough.
    pending_notifications: VecDeque<(&'static str, id, Instant)>,
    /// Backing MPMoviePlayerController objects owned by
    /// MPMoviePlayerViewController.
    view_controller_players: HashMap<id, id>,
}
impl State {
    fn get(env: &mut Environment) -> &mut Self {
        &mut env.framework_state.media_player.movie_player
    }
}

type MPMovieScalingMode = NSInteger;
type MPMovieControlStyle = NSInteger;
type MPMovieRepeatMode = NSInteger;

type MPMoviePlaybackState = NSInteger;
const MPMoviePlaybackStateStopped: MPMoviePlaybackState = 0;
const MPMoviePlaybackStatePlaying: MPMoviePlaybackState = 1;
const MPMoviePlaybackStatePaused: MPMoviePlaybackState = 2;

const FALLBACK_MOVIE_NATURAL_SIZE: CGSize = CGSize {
    width: 480.0,
    height: 320.0,
};

// Values might not be correct, but as these are linked symbol constants, it
// shouldn't matter.
pub const MPMoviePlayerPlaybackDidFinishNotification: &str =
    "MPMoviePlayerPlaybackDidFinishNotification";
/// Apparently an undocumented, private API. Spore Origins uses it.
pub const MPMoviePlayerContentPreloadDidFinishNotification: &str =
    "MPMoviePlayerContentPreloadDidFinishNotification";
pub const MPMoviePlayerScalingModeDidChangeNotification: &str =
    "MPMoviePlayerScalingModeDidChangeNotification";
pub const MPMoviePlayerPlaybackStateDidChangeNotification: &str =
    "MPMoviePlayerPlaybackStateDidChangeNotification";
pub const MPMoviePlayerLoadStateDidChangeNotification: &str =
    "MPMoviePlayerLoadStateDidChangeNotification";
pub const MPMediaPlaybackIsPreparedToPlayDidChangeNotification: &str =
    "MPMediaPlaybackIsPreparedToPlayDidChangeNotification";
// TODO: More notifications?
const MPMoviePlayerPlaybackDidFinishReasonUserInfoKey: &str =
    "MPMoviePlayerPlaybackDidFinishReasonUserInfoKey";

/// `NSNotificationName` values and other constants.
pub const CONSTANTS: ConstantExports = &[
    (
        "_MPMoviePlayerPlaybackDidFinishNotification",
        HostConstant::NSString(MPMoviePlayerPlaybackDidFinishNotification),
    ),
    (
        "_MPMoviePlayerContentPreloadDidFinishNotification",
        HostConstant::NSString(MPMoviePlayerContentPreloadDidFinishNotification),
    ),
    (
        "_MPMoviePlayerScalingModeDidChangeNotification",
        HostConstant::NSString(MPMoviePlayerScalingModeDidChangeNotification),
    ),
    (
        "_MPMoviePlayerPlaybackStateDidChangeNotification",
        HostConstant::NSString(MPMoviePlayerPlaybackStateDidChangeNotification),
    ),
    (
        "_MPMoviePlayerLoadStateDidChangeNotification",
        HostConstant::NSString(MPMoviePlayerLoadStateDidChangeNotification),
    ),
    (
        "_MPMediaPlaybackIsPreparedToPlayDidChangeNotification",
        HostConstant::NSString(MPMediaPlaybackIsPreparedToPlayDidChangeNotification),
    ),
    (
        "_MPMoviePlayerPlaybackDidFinishReasonUserInfoKey",
        HostConstant::NSString(MPMoviePlayerPlaybackDidFinishReasonUserInfoKey),
    ),
];

struct MPMoviePlayerControllerHostObject {
    // NSURL *
    content_url: id,
    // UIView *
    view: id,
    natural_size: CGSize,
    fullscreen: bool,
    repeat_mode: MPMovieRepeatMode,
    should_autoplay: bool,
    playback_state: MPMoviePlaybackState,
    current_playback_time: NSTimeInterval,
    duration: NSTimeInterval,
    prepared: bool,
    #[cfg(target_os = "android")]
    decoder: Option<MovieDecoder>,
    #[cfg(target_os = "android")]
    playback_started_at: Option<Instant>,
    #[cfg(target_os = "android")]
    playback_start_position: NSTimeInterval,
    #[cfg(target_os = "android")]
    raised_movie_container: bool,
    #[cfg(target_os = "android")]
    original_movie_z: Option<CGFloat>,
}
impl HostObject for MPMoviePlayerControllerHostObject {}

#[cfg(target_os = "android")]
fn load_android_movie(env: &mut Environment, player: id) -> bool {
    let (url, loaded) = {
        let host = env.objc.borrow::<MPMoviePlayerControllerHostObject>(player);
        (host.content_url, host.decoder.is_some())
    };
    if loaded {
        return true;
    }
    if url == nil {
        return false;
    }
    let path = ns_url::to_rust_path(env, url);
    let movie = match env.fs.read(path.as_ref()) {
        Ok(movie) => movie,
        Err(()) => {
            log!(
                "Android movie file not found in guest filesystem: {:?}",
                path
            );
            return false;
        }
    };
    let decoder = match MovieDecoder::open(&movie) {
        Ok(decoder) => decoder,
        Err(err) => {
            log!("Android MediaCodec movie preparation failed: {err}");
            return false;
        }
    };
    let size = CGSize {
        width: decoder.width as f32,
        height: decoder.height as f32,
    };
    let duration = decoder.duration_us as f64 / 1_000_000.0;
    log!(
        "Android video prepared: {:?}, {}x{}, duration {:.3}s",
        path,
        decoder.width,
        decoder.height,
        duration
    );
    let host = env
        .objc
        .borrow_mut::<MPMoviePlayerControllerHostObject>(player);
    host.natural_size = size;
    if duration > 0.0 {
        host.duration = duration;
    }
    host.decoder = Some(decoder);
    true
}

// Keep the Create Save dialog above the title movie in Hunters 2.
#[cfg(target_os = "android")]
pub(super) fn set_hunters_create_save_visible(
    env: &mut Environment,
    visible: bool,
    controller: id,
) {
    // Only record state inside objc_msgSend. Sending a new Objective-C
    // message here corrupted the guest registers in earlier builds.
    let state = State::get(env);
    state.hunters_create_save_visible = visible;
    if visible {
        state.hunters_create_save_controller = Some(controller);
    }
    log!("Hunters 2 Create Save overlay visible={visible}; depth change deferred");
}

// This is recorded during Objective-C dispatch. Do not send any Objective-C
// messages here: earlier nested sends corrupted the guest register state.
#[cfg(target_os = "android")]
pub(super) fn set_hunters_message_visible(env: &mut Environment, visible: bool, controller: id) {
    let state = State::get(env);
    state.hunters_message_visible = visible;
    if visible {
        state.hunters_message_controller = Some(controller);
    }
    log!("Hunters 2 confirmation overlay visible={visible}; depth change deferred");
}

// The resume-contract prompt appears for existing save slots. It must be
// displayed above the menu video, just like the Create Save popup.
#[cfg(target_os = "android")]
pub(super) fn set_hunters_resume_contract_visible(
    env: &mut Environment,
    visible: bool,
    controller: id,
) {
    let state = State::get(env);
    state.hunters_resume_contract_visible = visible;
    if visible {
        state.hunters_resume_contract_controller = Some(controller);
    }
    log!("Hunters 2 Resume Contract overlay visible={visible}; depth deferred");
}

// Called from the guest message-dispatch path: only record state here.
// All Core Animation mutations happen later during the host video tick.
#[cfg(target_os = "android")]
pub(super) fn set_hunters_gameplay_transition(env: &mut Environment, started: bool) {
    State::get(env).hunters_gameplay_transition = started;
    log!("Hunters 2 gameplay transition: started={started}; movie depth change deferred");
}

// Change drawing depth without modifying the game's logical subview order.
#[cfg(target_os = "android")]
fn restore_hunters_movie_overlay(env: &mut Environment, view: id, original_z: CGFloat) {
    if view == nil || env.bundle.bundle_identifier() != "uk.co.rodeogames.hunterstwo" {
        return;
    }
    let container: id = msg![env; view superview];
    if container != nil {
        let layer: id = msg![env; container layer];
        let depth = if State::get(env).hunters_gameplay_transition {
            -1000.0
        } else {
            original_z
        };
        () = msg![env; layer setZPosition:depth];
        log!("Hunters 2 movie overlay: restored layer {layer:?} z={depth}");
    }
}

// Raise the Create Save overlay's rendering branch above the title movie.
// Keep UIKit's view hierarchy untouched, and run this outside guest dispatch.
#[cfg(target_os = "android")]
fn update_hunters_create_save_depth(env: &mut Environment, movie_view: id) {
    if env.bundle.bundle_identifier() != "uk.co.rodeogames.hunterstwo" {
        return;
    }
    if !State::get(env).hunters_create_save_visible {
        let raised = State::get(env).hunters_raised_create_save_layer.take();
        if let Some((layer, old_z)) = raised {
            () = msg![env; layer setZPosition:old_z];
            log!("Hunters 2 Create Save layer restored: layer={layer:?}, z={old_z}");
        }
        return;
    }
    if State::get(env).hunters_raised_create_save_layer.is_some() {
        return;
    }
    let Some(controller) = State::get(env).hunters_create_save_controller else {
        return;
    };
    let overlay_view: id = msg![env; controller view];
    let movie_parent: id = msg![env; movie_view superview];
    if overlay_view == nil || movie_parent == nil {
        log_once!("Hunters 2 Create Save: overlay view or movie parent missing");
        return;
    }
    let movie_container: id = msg![env; movie_parent layer];
    let movie_parent_layer: id = msg![env; movie_container superlayer];
    if movie_parent_layer == nil {
        return;
    }
    let overlay_layer: id = msg![env; overlay_view layer];
    // Find the overlay branch sharing the movie container's parent.
    let mut branch = overlay_layer;
    let mut found = false;
    for _ in 0..32 {
        let parent: id = msg![env; branch superlayer];
        if parent == movie_parent_layer {
            found = true;
            break;
        }
        if parent == nil {
            break;
        }
        branch = parent;
    }
    if !found {
        log_once!("Hunters 2 Create Save: overlay layer has no shared movie parent");
        return;
    }
    // If the overlay lives inside the movie container, raise the direct child
    // containing the overlay, rather than the entire movie container.
    let layer = if branch == movie_container {
        let mut child = overlay_layer;
        for _ in 0..32 {
            let parent: id = msg![env; child superlayer];
            if parent == movie_container || parent == nil {
                break;
            }
            child = parent;
        }
        child
    } else {
        branch
    };
    if layer == movie_container {
        return;
    }
    let old_z: CGFloat = msg![env; layer zPosition];
    () = msg![env; layer setZPosition:2000.0f32];
    State::get(env).hunters_raised_create_save_layer = Some((layer, old_z));
    let window: id = msg![env; overlay_view window];
    let hidden: bool = msg![env; overlay_view isHidden];
    let alpha: CGFloat = msg![env; overlay_view alpha];
    let frame: CGRect = msg![env; overlay_view frame];
    log!(
        "Hunters 2 Create Save raised: view={overlay_view:?}, window={window:?}, hidden={hidden}, alpha={alpha}, frame={frame:?}, layer={layer:?}, old_z={old_z}, new_z=2000"
    );
}

// Confirmation dialogs use OverlayMessage, not OverlayCreateSave. Restore
// their original layer depth once dismissed and raise only their own branch
// while visible, leaving the actual UIKit view ordering and input unchanged.
#[cfg(target_os = "android")]
fn update_hunters_message_depth(env: &mut Environment, movie_view: id) {
    if env.bundle.bundle_identifier() != "uk.co.rodeogames.hunterstwo" {
        return;
    }
    if !State::get(env).hunters_message_visible {
        if let Some((layer, old_z)) = State::get(env).hunters_raised_message_layer.take() {
            () = msg![env; layer setZPosition:old_z];
            log!("Hunters 2 message layer restored: layer={layer:?}, z={old_z}");
        }
        return;
    }
    if State::get(env).hunters_raised_message_layer.is_some() {
        return;
    }
    let Some(controller) = State::get(env).hunters_message_controller else {
        return;
    };
    let overlay_view: id = msg![env; controller view];
    let movie_container_view: id = msg![env; movie_view superview];
    if overlay_view == nil || movie_container_view == nil {
        return;
    }
    let movie_container: id = msg![env; movie_container_view layer];
    let shared_parent: id = msg![env; movie_container superlayer];
    if shared_parent == nil {
        return;
    }
    let overlay_layer: id = msg![env; overlay_view layer];
    let mut branch = overlay_layer;
    let mut found = false;
    for _ in 0..32 {
        let parent: id = msg![env; branch superlayer];
        if parent == shared_parent {
            found = true;
            break;
        }
        if parent == nil {
            break;
        }
        branch = parent;
    }
    if !found {
        log_once!("Hunters 2 message layer has no shared movie parent");
        return;
    }
    // Avoid bringing the movie container itself forward. The dialog might
    // be nested inside that container; in that case raise its direct child.
    let layer = if branch == movie_container {
        let mut child = overlay_layer;
        for _ in 0..32 {
            let parent: id = msg![env; child superlayer];
            if parent == movie_container || parent == nil {
                break;
            }
            child = parent;
        }
        child
    } else {
        branch
    };
    if layer == movie_container {
        return;
    }
    let old_z: CGFloat = msg![env; layer zPosition];
    () = msg![env; layer setZPosition:2000.0f32];
    State::get(env).hunters_raised_message_layer = Some((layer, old_z));
    log!(
        "Hunters 2 message layer raised: controller={controller:?}, layer={layer:?}, old_z={old_z}, new_z=2000"
    );
}

// Existing save slots show OverlayResumeContract, not OverlayMessage.
// Its rendering branch otherwise stays behind the video (z=1000).
// Do not alter the UIKit view hierarchy or the dialog's input handling.
#[cfg(target_os = "android")]
fn update_hunters_resume_contract_depth(env: &mut Environment, movie_view: id) {
    if env.bundle.bundle_identifier() != "uk.co.rodeogames.hunterstwo" {
        return;
    }
    if !State::get(env).hunters_resume_contract_visible {
        if let Some((layer, old_z)) = State::get(env).hunters_raised_resume_contract_layer.take() {
            () = msg![env; layer setZPosition:old_z];
            log!("Hunters 2 Resume Contract restored: layer={layer:?}, z={old_z}");
        }
        return;
    }
    if State::get(env)
        .hunters_raised_resume_contract_layer
        .is_some()
    {
        return;
    }
    let Some(controller) = State::get(env).hunters_resume_contract_controller else {
        return;
    };
    let overlay_view: id = msg![env; controller view];
    let movie_container_view: id = msg![env; movie_view superview];
    if overlay_view == nil || movie_container_view == nil {
        return;
    }
    let movie_container: id = msg![env; movie_container_view layer];
    let shared_parent: id = msg![env; movie_container superlayer];
    if shared_parent == nil {
        return;
    }
    let overlay_layer: id = msg![env; overlay_view layer];
    let mut branch = overlay_layer;
    let mut found = false;
    for _ in 0..32 {
        let parent: id = msg![env; branch superlayer];
        if parent == shared_parent {
            found = true;
            break;
        }
        if parent == nil {
            break;
        }
        branch = parent;
    }
    if !found {
        log_once!("Hunters 2 Resume Contract: no shared movie parent");
        return;
    }
    let layer = if branch == movie_container {
        let mut child = overlay_layer;
        for _ in 0..32 {
            let parent: id = msg![env; child superlayer];
            if parent == movie_container || parent == nil {
                break;
            }
            child = parent;
        }
        child
    } else {
        branch
    };
    if layer == movie_container {
        return;
    }
    let old_z: CGFloat = msg![env; layer zPosition];
    () = msg![env; layer setZPosition:2000.0f32];
    State::get(env).hunters_raised_resume_contract_layer = Some((layer, old_z));
    let hidden: bool = msg![env; overlay_view isHidden];
    let alpha: CGFloat = msg![env; overlay_view alpha];
    let rect: CGRect = msg![env; overlay_view frame];
    log!(
        "Hunters 2 Resume Contract raised: controller={controller:?},          layer={layer:?}, old_z={old_z}, new_z=2000, hidden={hidden},          alpha={alpha:.2}, frame={rect:?}"
    );
}

#[cfg(target_os = "android")]
fn movie_video_tick(env: &mut Environment) {
    let Some(player) = State::get(env).active_player else {
        return;
    };
    // Apply the overlay depth adjustment from the host tick, never from
    // objc_msgSend. Keep the movie raised until playback is actually stopped.
    let movie_view = env
        .objc
        .borrow::<MPMoviePlayerControllerHostObject>(player)
        .view;
    update_hunters_create_save_depth(env, movie_view);
    update_hunters_message_depth(env, movie_view);
    update_hunters_resume_contract_depth(env, movie_view);
    // Hunters 2 can leave the title video looping after a successful
    // Create Save callback. Keep decoding it for game notifications, but
    // render behind the game instead of covering the next scene.
    if State::get(env).hunters_gameplay_transition && movie_view != nil {
        let movie_container: id = msg![env; movie_view superview];
        if movie_container != nil {
            let layer: id = msg![env; movie_container layer];
            let z: CGFloat = msg![env; layer zPosition];
            if z != -1000.0 {
                let background_z: CGFloat = -1000.0;
                () = msg![env; layer setZPosition:background_z];
                log!("Hunters 2 gameplay video depth: layer={layer:?}, old_z={z}, new_z=-1000");
            }
        }
    }
    let (frame, view, ending, looping, error) = {
        let host = env
            .objc
            .borrow_mut::<MPMoviePlayerControllerHostObject>(player);
        if host.playback_state != MPMoviePlaybackStatePlaying {
            return;
        }
        let Some(decoder) = host.decoder.as_mut() else {
            return;
        };
        let now = Instant::now();
        let elapsed = host
            .playback_started_at
            .map(|since| now.saturating_duration_since(since).as_secs_f64())
            .unwrap_or(0.0);
        let time = host.playback_start_position + elapsed;
        host.current_playback_time = time;
        let frame = decoder.frame_for_time((time.max(0.0) * 1_000_000.0) as i64);
        let finished = decoder.is_finished() || (host.duration > 0.0 && time >= host.duration);
        let looping = finished && host.repeat_mode == 1;
        let ending = finished && !looping;
        if looping {
            match decoder.restart(0) {
                Ok(()) => {
                    host.playback_started_at = Some(now);
                    host.playback_start_position = 0.0;
                    host.current_playback_time = 0.0;
                }
                Err(err) => {
                    log!("Android movie loop seek failed: {err}");
                    return;
                }
            }
        }
        match frame {
            Ok(frame) => (frame, host.view, ending, looping, None),
            Err(err) => (None, host.view, true, looping, Some(err)),
        }
    };
    if let Some(err) = error {
        log!("Android movie decoding stopped: {err}");
    }
    if let Some(Frame {
        mut pixels,
        width,
        height,
        time_us: _,
    }) = frame
    {
        if view != nil {
            let is_hunters_2 = env.bundle.bundle_identifier() == "uk.co.rodeogames.hunterstwo";
            if is_hunters_2 {
                // MediaCodec emits top-first RGBA, while this CALayer's
                // texture coordinates treat the first row as the bottom.
                let stride = width as usize * 4;
                let rows = height as usize;
                for top_row in 0..rows / 2 {
                    let bottom_row = rows - 1 - top_row;
                    let (top, bottom) = pixels.split_at_mut(bottom_row * stride);
                    let start = top_row * stride;
                    top[start..start + stride].swap_with_slice(&mut bottom[..stride]);
                }
            }
            // Adjust drawing depth only; reordering UIKit subviews breaks
            // Hunters 2's overlay manager during Create Save.
            if is_hunters_2
                && !State::get(env).hunters_create_save_visible
                && !State::get(env).hunters_gameplay_transition
            {
                let needs_raise = {
                    let host = env
                        .objc
                        .borrow_mut::<MPMoviePlayerControllerHostObject>(player);
                    let needs_raise = !host.raised_movie_container;
                    host.raised_movie_container = true;
                    needs_raise
                };
                if needs_raise {
                    let container: id = msg![env; view superview];
                    if container != nil {
                        let layer: id = msg![env; container layer];
                        let old_z: CGFloat = msg![env; layer zPosition];
                        () = msg![env; layer setZPosition:1000.0f32];
                        let host = env
                            .objc
                            .borrow_mut::<MPMoviePlayerControllerHostObject>(player);
                        host.original_movie_z = Some(old_z);
                        () = msg![env; view setUserInteractionEnabled:false];
                        log!(
                            "Hunters 2 video render depth: layer={layer:?}, old_z={old_z}, new_z=1000"
                        );
                    }
                }
            }
            let layer: id = msg![env; view layer];
            static VIDEO_FRAME_COUNT: std::sync::atomic::AtomicUsize =
                std::sync::atomic::AtomicUsize::new(0);
            let frame_number =
                VIDEO_FRAME_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if matches!(frame_number, 1 | 15 | 30 | 90 | 180) {
                let bright = pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|rgba| rgba[0] > 32 || rgba[1] > 32 || rgba[2] > 32)
                    .count();
                let rect: CGRect = msg![env; view frame];
                let superview: id = msg![env; view superview];
                let window: id = msg![env; view window];
                let hidden: bool = msg![env; view isHidden];
                let opacity: f32 = msg![env; layer opacity];
                log!(
                    "Android video frame {frame_number}: {}x{}, bright={bright}/{}, view={view:?}, frame={rect:?}, superview={superview:?}, window={window:?}, hidden={hidden}, opacity={opacity:.2}",
                    width, height, pixels.len() / 4,
                );
            }
            present_movie_pixels(env, layer, pixels, width, height);
        } else {
            log_once!("Android movie frame decoded but player has no view");
        }
    }
    if ending && !looping {
        let is_hunters_2 = env.bundle.bundle_identifier() == "uk.co.rodeogames.hunterstwo";
        let pending = &mut State::get(env).pending_notifications;
        if !pending.iter().any(|(name, obj, _)| {
            *name == MPMoviePlayerPlaybackDidFinishNotification && *obj == player
        }) {
            if is_hunters_2 {
                log!("Hunters 2 movie playback reached end: player={player:?}");
            }
            pending.push_back((
                MPMoviePlayerPlaybackDidFinishNotification,
                player,
                Instant::now(),
            ));
        }
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation MPMoviePlayerController: NSObject

// TODO: actual playback

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(MPMoviePlayerControllerHostObject {
        content_url: nil,
        view: nil,
        natural_size: FALLBACK_MOVIE_NATURAL_SIZE,
        fullscreen: false,
        repeat_mode: 0,
        should_autoplay: true,
        playback_state: MPMoviePlaybackStateStopped,
        current_playback_time: 0.0,
        duration: 1.0,
        prepared: false,
        #[cfg(target_os = "android")]
        decoder: None,
        #[cfg(target_os = "android")]
        playback_started_at: None,
        #[cfg(target_os = "android")]
        playback_start_position: 0.0,
        #[cfg(target_os = "android")]
        raised_movie_container: false,
        #[cfg(target_os = "android")]
        original_movie_z: None,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)init {
    let needs_view = env
        .objc
        .borrow::<MPMoviePlayerControllerHostObject>(this)
        .view == nil;
    if needs_view {
        let natural_size = env
            .objc
            .borrow::<MPMoviePlayerControllerHostObject>(this)
            .natural_size;
        let frame = CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: natural_size,
        };
        let view: id = msg_class![env; UIView alloc];
        let view: id = msg![env; view initWithFrame:frame];
        env.objc
            .borrow_mut::<MPMoviePlayerControllerHostObject>(this)
            .view = view;
    }
    this
}

- (id)initWithContentURL:(id)url { // NSURL*
    if url == nil {
        log!(
            "TODO: [(MPMoviePlayerController*){:?} initWithContentURL:nil]",
            this,
        );
    } else {
        log!(
            "TODO: [(MPMoviePlayerController*){:?} initWithContentURL:{:?} ({:?})]",
            this,
            url,
            ns_url::to_rust_path(env, url),
        );
    }

    let this: id = msg![env; this init];
    if this == nil {
        return nil;
    }
    let _: () = msg![env; this setContentURL:url];

    // Act as if loading immediately completed (Spore Origins waits for this).
    State::get(env).pending_notifications.push_back(
        (MPMoviePlayerContentPreloadDidFinishNotification, this, Instant::now())
    );

    this
}

- (())dealloc {
    let (url, view) = {
        let host = env.objc.borrow::<MPMoviePlayerControllerHostObject>(this);
        (host.content_url, host.view)
    };
    release(env, url);
    release(env, view);

    env.objc.dealloc_object(this, &mut env.mem);
}

- (id)contentURL {
    env.objc.borrow::<MPMoviePlayerControllerHostObject>(this).content_url
}

- (())setContentURL:(id)url {
    if url != nil {
        retain(env, url);
    }
    let old_url = {
        let host = env
            .objc
            .borrow_mut::<MPMoviePlayerControllerHostObject>(this);
        let old_url = host.content_url;
        host.content_url = url;
        host.current_playback_time = 0.0;
        host.playback_state = MPMoviePlaybackStateStopped;
        host.prepared = false;
        #[cfg(target_os = "android")]
        {
            host.decoder = None;
            host.playback_started_at = None;
            host.playback_start_position = 0.0;
        }
        old_url
    };
    if old_url != nil {
        release(env, old_url);
    }
}

- (id)backgroundColor {
    msg_class![env; UIColor blackColor] // TODO
}
- (())setBackgroundColor:(id)color { // UIColor*
    todo_objc_setter!(this, color);
}

- (())setScalingMode:(MPMovieScalingMode)mode {
    todo_objc_setter!(this, mode);
}
- (())setUseApplicationAudioSession:(bool)use_session {
    todo_objc_setter!(this, use_session);
}
- (())setControlStyle:(MPMovieControlStyle)style {
    todo_objc_setter!(this, style);
}
- (())setFullscreen:(bool)fullscreen {
    env.objc
        .borrow_mut::<MPMoviePlayerControllerHostObject>(this)
        .fullscreen = fullscreen;
}
- (())setFullscreen:(bool)fullscreen animated:(bool)_animated {
    let _: () = msg![env; this setFullscreen:fullscreen];
}
- (())setRepeatMode:(MPMovieRepeatMode)repeat_mode {
    env.objc
        .borrow_mut::<MPMoviePlayerControllerHostObject>(this)
        .repeat_mode = repeat_mode;
}
- (())setShouldAutoplay:(bool)should_autoplay {
    env.objc
        .borrow_mut::<MPMoviePlayerControllerHostObject>(this)
        .should_autoplay = should_autoplay;
}
- (())setInitialPlaybackTime:(NSTimeInterval)initial_time {
    env.objc
        .borrow_mut::<MPMoviePlayerControllerHostObject>(this)
        .current_playback_time = initial_time;
}

- (NSTimeInterval)currentPlaybackTime {
    env.objc
        .borrow::<MPMoviePlayerControllerHostObject>(this)
        .current_playback_time
}
- (())setCurrentPlaybackTime:(NSTimeInterval)time {
    env.objc
        .borrow_mut::<MPMoviePlayerControllerHostObject>(this)
        .current_playback_time = time;
    #[cfg(target_os = "android")]
    {
        let host = env.objc.borrow_mut::<MPMoviePlayerControllerHostObject>(this);
        if let Some(decoder) = host.decoder.as_mut() {
            if let Err(err) = decoder.restart((time.max(0.0) * 1_000_000.0) as i64) {
                log!("Android movie seek failed: {err}");
            }
        }
        host.playback_start_position = time;
        host.playback_started_at = Some(Instant::now());
    }
}
- (NSTimeInterval)duration {
    env.objc
        .borrow::<MPMoviePlayerControllerHostObject>(this)
        .duration
}
- (f32)currentPlaybackRate {
    let state = env
        .objc
        .borrow::<MPMoviePlayerControllerHostObject>(this)
        .playback_state;
    if state == MPMoviePlaybackStatePlaying {
        1.0
    } else {
        0.0
    }
}

- (id)view {
    env.objc.borrow::<MPMoviePlayerControllerHostObject>(this).view
}

- (CGSize)naturalSize {
    env.objc
        .borrow::<MPMoviePlayerControllerHostObject>(this)
        .natural_size
}

- (())prepareToPlay {
    #[cfg(target_os = "android")]
    load_android_movie(env, this);
    env.objc
        .borrow_mut::<MPMoviePlayerControllerHostObject>(this)
        .prepared = true;
    let now = Instant::now();
    State::get(env).pending_notifications.push_back((
        MPMoviePlayerLoadStateDidChangeNotification,
        this,
        now,
    ));
    State::get(env).pending_notifications.push_back((
        MPMediaPlaybackIsPreparedToPlayDidChangeNotification,
        this,
        now,
    ));
}

- (MPMoviePlaybackState)playbackState {
    env.objc
        .borrow::<MPMoviePlayerControllerHostObject>(this)
        .playback_state
}

// Apparently an undocumented, private API, but Spore Origins uses it.
- (())setMovieControlMode:(NSInteger)_mode {
    // As this is undocumented and we don't have real video playback yet, let's
    // ignore it.
}

// Another undocumented one! But some apps may still use it :/
// https://stackoverflow.com/a/1390079/2241008
- (())setOrientation:(UIDeviceOrientation)_orientation animated:(bool)_animated {

}

// MPMediaPlayback implementation
- (())play {
    if env.bundle.bundle_identifier() == "uk.co.rodeogames.hunterstwo" {
        log!("Hunters 2 movie playback play requested: player={this:?}");
    }
    #[cfg(target_os = "android")]
    let video_available = load_android_movie(env, this);
    #[cfg(not(target_os = "android"))]
    let video_available = false;
    if !video_available {
        log!("TODO: [(MPMoviePlayerController*){:?} play] - using simulated playback", this);
    }
    // A player may be paused (or reset to Stopped by setContentURL:)
    // while it still owns the runtime's single active-player retain.
    // Hunters 2 replays that exact player when returning to the ship hub.
    // Do not assert that the active slot is empty or take a second retain.
    let mut reuse_hunters_active_player = false;
    if let Some(old) = env.framework_state.media_player.movie_player.active_player {
        if old == this {
            let playback_state = env
                .objc
                .borrow::<MPMoviePlayerControllerHostObject>(this)
                .playback_state;
            if playback_state == MPMoviePlaybackStatePlaying {
                return;
            }
            if cfg!(target_os = "android")
                && env.bundle.bundle_identifier() == "uk.co.rodeogames.hunterstwo"
            {
                reuse_hunters_active_player = true;
                log!(
                    "Hunters 2: resuming existing active movie player={this:?}, previous_state={playback_state}"
                );
            }
            // The initial active-player retain is still held.
        } else {
            let _: () = msg![env; old stop];
        }
    }
    assert!(
        reuse_hunters_active_player
            || env.framework_state.media_player.movie_player.active_player.is_none()
    );
    {
        let host = env.objc.borrow_mut::<MPMoviePlayerControllerHostObject>(this);
        #[cfg(target_os = "android")]
        if video_available {
            if host.playback_state == MPMoviePlaybackStateStopped {
                host.current_playback_time = 0.0;
                if let Some(decoder) = host.decoder.as_mut() {
                    if let Err(err) = decoder.restart(0) {
                        log!("Android movie replay seek failed: {err}");
                    }
                }
            }
            host.playback_start_position = host.current_playback_time;
            host.playback_started_at = Some(Instant::now());
        }
        host.playback_state = MPMoviePlaybackStatePlaying;
    }
    State::get(env).pending_notifications.push_back((
        MPMoviePlayerPlaybackStateDidChangeNotification,
        this,
        Instant::now(),
    ));
    // Movie player is retained by the runtime until it is stopped.
    if env.framework_state.media_player.movie_player.active_player != Some(this) {
        retain(env, this);
    }
    env.framework_state.media_player.movie_player.active_player = Some(this);

    if video_available {
        // Decode real frames; completion is posted by movie_video_tick().
        return;
    }

    // Act as if playback immediately completed after 1 second
    // (various apps wait for this, such as BIA and Hero of Sparta).
    let notif = (MPMoviePlayerPlaybackDidFinishNotification, this, Instant::now().checked_add(Duration::from_millis(1000)).unwrap());
    for (name, obj, _) in &mut State::get(env).pending_notifications {
        // De-duplicate similar notifications. This can happen if app is calling
        // `play` twice on the same player object (case of NOVA2).
        if *name == MPMoviePlayerPlaybackDidFinishNotification && *obj == this {
            return;
        }
    }
    State::get(env).pending_notifications.push_back(notif);
}

- (())pause {
    let host = env.objc.borrow_mut::<MPMoviePlayerControllerHostObject>(this);
    #[cfg(target_os = "android")]
    if let Some(started) = host.playback_started_at.take() {
        host.current_playback_time = host.playback_start_position
            + Instant::now().saturating_duration_since(started).as_secs_f64();
    }
    host.playback_state = MPMoviePlaybackStatePaused;
    State::get(env).pending_notifications.push_back((
        MPMoviePlayerPlaybackStateDidChangeNotification,
        this,
        Instant::now(),
    ));
}

- (())stop {
    if env.bundle.bundle_identifier() == "uk.co.rodeogames.hunterstwo" {
        log!("Hunters 2 movie playback stop requested: player={this:?}");
    }
    let host = env.objc.borrow_mut::<MPMoviePlayerControllerHostObject>(this);
    host.playback_state = MPMoviePlaybackStateStopped;
    #[cfg(target_os = "android")]
    {
        host.playback_started_at = None;
        host.playback_start_position = 0.0;
        host.current_playback_time = 0.0;
        if let Some(decoder) = host.decoder.as_mut() {
            if let Err(err) = decoder.restart(0) {
                log!("Android movie rewind failed: {err}");
            }
        }
    }
    #[cfg(target_os = "android")]
    let restore_view = if std::mem::take(&mut host.raised_movie_container) {
        Some((host.view, host.original_movie_z.take().unwrap_or(0.0)))
    } else {
        None
    };
    #[cfg(target_os = "android")]
    if let Some((view, original_z)) = restore_view {
        restore_hunters_movie_overlay(env, view, original_z);
    }
    State::get(env).pending_notifications.push_back((
        MPMoviePlayerPlaybackStateDidChangeNotification,
        this,
        Instant::now(),
    ));
    if env.framework_state.media_player.movie_player.active_player == Some(this) {
        // Some applications (like NOVA2) may send 2 `stop` messages for each
        // 1 `play` message for the player. In that case, we want to release
        // the active player only once.
        env.framework_state.media_player.movie_player.active_player = None;
        release(env, this);
    }
}

@end

@implementation MPMoviePlayerViewController: UIViewController

- (id)initWithContentURL:(id)url {
    log!(
        "TODO: [(MPMoviePlayerViewController*){:?} initWithContentURL:{:?} ({:?})]",
        this,
        url,
        ns_url::to_rust_path(env, url),
    );

    let this: id = msg_super![env; this initWithNibName:nil bundle:nil];
    if this == nil {
        return nil;
    }

    let player: id = msg_class![env; MPMoviePlayerController alloc];
    let player: id = msg![env; player initWithContentURL:url];
    if player == nil {
        release(env, this);
        return nil;
    }

    let view: id = msg![env; player view];
    () = msg![env; this setView:view];

    if let Some(old_player) = State::get(env).view_controller_players.insert(this, player) {
        release(env, old_player);
    }

    this
}

- (id)moviePlayer {
    if let Some(player) = State::get(env)
        .view_controller_players
        .get(&this)
        .copied()
    {
        return player;
    }

    // Some apps instantiate MPMoviePlayerViewController subclasses from a nib
    // and expect the moviePlayer property to already exist. Create it lazily
    // so those subclasses behave like the real framework.
    log!(
        "Creating lazy MPMoviePlayerController for view controller {:?}",
        this
    );
    let player: id = msg_class![env; MPMoviePlayerController alloc];
    let player: id = msg![env; player init];
    if player != nil {
        State::get(env).view_controller_players.insert(this, player);
    }
    player
}

- (())dealloc {
    if let Some(player) = State::get(env).view_controller_players.remove(&this) {
        release(env, player);
    }
    let _: () = msg_super![env; this dealloc];
}

@end

};

/// Schedule recovery of a stalled save-menu unload. No guest message is
/// dispatched from within GameController.onCoreViewReadyToUnload.
#[cfg(target_os = "android")]
pub(super) fn queue_hunters_save_menu_unload(
    env: &mut Environment,
    game_controller: id,
    save_menu: id,
) {
    if env.bundle.bundle_identifier() != "uk.co.rodeogames.hunterstwo" {
        return;
    }
    State::get(env).hunters_pending_save_unload =
        Some((game_controller, save_menu, Instant::now()));
    log!("Hunters 2: queued SaveMenuController unload recovery for {save_menu:?}");
}

/// Remember that the ship screen is awaiting its normal onLoad callback.
#[cfg(target_os = "android")]
pub(super) fn queue_hunters_ship_load(env: &mut Environment, game: id, ship: id) {
    if env.bundle.bundle_identifier() != "uk.co.rodeogames.hunterstwo" {
        return;
    }
    State::get(env).hunters_pending_ship_load = Some((game, ship, Instant::now()));
    log!("Hunters 2: queued ShipGameController load recovery for {ship:?}");
}

/// An actual onLoad event always wins over recovery.
#[cfg(target_os = "android")]
pub(super) fn hunters_ship_load_started(env: &mut Environment, ship: id) {
    if State::get(env)
        .hunters_pending_ship_load
        .is_some_and(|(_, pending, _)| pending == ship)
    {
        State::get(env).hunters_pending_ship_load = None;
        log!("Hunters 2: ShipGameController entered onLoad; recovery cancelled");
    }
}

/// Mark the safe handoff point without dispatching Objective-C messages
/// inside GameController's own Objective-C method invocation.
#[cfg(target_os = "android")]
pub(super) fn hunters_replacement_scene_displayed(env: &mut Environment) {
    let state = State::get(env);
    if !state.hunters_completed_player_holds.is_empty() {
        state.hunters_replacement_scene_displayed = true;
        log!("Hunters 2: replacement CoreView displayed; finished-player cleanup queued");
    }
}

/// For use by `NSRunLoop` via [super::handle_players]: check movie players'
/// status, send notifications if necessary.
pub(super) fn handle_players(env: &mut Environment) {
    #[cfg(target_os = "android")]
    movie_video_tick(env);
    let mut notifs_to_run = Vec::new();
    let pending_notifs = &mut State::get(env).pending_notifications;
    let mut i = 0;
    while i < pending_notifs.len() {
        let (name_str, object, time) = pending_notifs[i];
        if Instant::now() >= time {
            notifs_to_run.push((name_str, object));
            pending_notifs.swap_remove_back(i);
        } else {
            i += 1;
        }
    }
    for (name_str, object) in notifs_to_run {
        // Playback completion is delivered synchronously to observers below.
        // Clear the active-player slot before the callback runs so an app can
        // start the next movie from its completion handler without `play`
        // trying to stop the just-finished player re-entrantly.
        let release_active_player = name_str == MPMoviePlayerPlaybackDidFinishNotification
            && State::get(env).active_player == Some(object);
        if release_active_player {
            State::get(env).active_player = None;
            let host = env
                .objc
                .borrow_mut::<MPMoviePlayerControllerHostObject>(object);
            host.playback_state = MPMoviePlaybackStateStopped;
            #[cfg(target_os = "android")]
            let restore_view = if std::mem::take(&mut host.raised_movie_container) {
                Some((host.view, host.original_movie_z.take().unwrap_or(0.0)))
            } else {
                None
            };
            #[cfg(target_os = "android")]
            if let Some((view, original_z)) = restore_view {
                restore_hunters_movie_overlay(env, view, original_z);
            }
        }

        if name_str == MPMoviePlayerPlaybackDidFinishNotification
            && env.bundle.bundle_identifier() == "uk.co.rodeogames.hunterstwo"
        {
            log!(
                "Hunters 2 movie completion notification: player={object:?}, active_was_released={release_active_player}"
            );
        }
        let name = ns_string::get_static_str(env, name_str);
        let center: id = msg_class![env; NSNotificationCenter defaultCenter];
        // TODO: should there be some user info attached?
        let _: () = msg![env; center postNotificationName:name object:object];

        // `play` takes one runtime retain while the player is active. Keep
        // that retain through the completion callback so the notification's
        // object stays valid, then release it after observers return.
        if release_active_player {
            // Hunters 2 still accesses the just-finished movie player
            // after IntroVideoController.onUnloadFinished, before the
            // new CoreView appears. Releasing the runtime's last retain
            // here causes a stale "retain" to crash objc_msgSend.
            #[cfg(target_os = "android")]
            if env.bundle.bundle_identifier() == "uk.co.rodeogames.hunterstwo" {
                State::get(env).hunters_completed_player_holds.push(object);
                log!(
                    "Hunters 2: holding completed movie player through CoreView handoff: {object:?}"
                );
            } else {
                release(env, object);
            }
            #[cfg(not(target_os = "android"))]
            release(env, object);
        }
    }

    // If the resume transition has been stuck for at least one second,
    // invoke SaveMenuController's *real* onUnload implementation, which
    // performs its cleanup and calls onUnloadFinished. Do not synthesize
    // status changes or bypass the game's controller lifecycle.
    #[cfg(target_os = "android")]
    if let Some((game, menu, queued_at)) = State::get(env).hunters_pending_save_unload {
        use crate::mem::ConstPtr;
        let game_addr = game.to_bits();
        let menu_addr = menu.to_bits();
        let status: u32 = env.mem.read(ConstPtr::from_bits(game_addr + 0xc8));
        let core: u32 = env.mem.read(ConstPtr::from_bits(game_addr + 0x98));
        let next_type: i32 = env.mem.read(ConstPtr::from_bits(game_addr + 0x9c));
        // A saved contract transitions to type 3 (DropGameController);
        // type 4 is the ship-hub path. In either case the old core must
        // still be the same SaveMenuController in loading status 3.
        if status != 3 || core != menu_addr || !matches!(next_type, 3 | 4) {
            State::get(env).hunters_pending_save_unload = None;
        } else if queued_at.elapsed() >= Duration::from_millis(1200) {
            State::get(env).hunters_pending_save_unload = None;
            let is_loaded: u8 = env.mem.read(ConstPtr::from_bits(menu_addr + 0xa3));
            if is_loaded != 0 {
                log!(
                    "Hunters 2: stalled resume; calling SaveMenuController.onUnload after timeout"
                );
                let _: () = msg![env; menu onUnload];
            }
        }
    }

    // Once the old save menu has unloaded, the ship hub may stall while
    // GameController remains in loading status 1. Run the game's actual
    // ShipGameController.onLoad once, but only after a grace period and
    // only if the same unloaded ship screen is still selected.
    #[cfg(target_os = "android")]
    if let Some((game, ship, queued_at)) = State::get(env).hunters_pending_ship_load {
        use crate::mem::ConstPtr;

        let game_addr = game.to_bits();
        let ship_addr = ship.to_bits();
        let status: u32 = env.mem.read(ConstPtr::from_bits(game_addr + 0xc8));
        let current_core: u32 = env.mem.read(ConstPtr::from_bits(game_addr + 0x98));
        let next_type: i32 = env.mem.read(ConstPtr::from_bits(game_addr + 0x9c));
        let is_loaded: u8 = env.mem.read(ConstPtr::from_bits(ship_addr + 0xa3));
        if current_core != ship_addr || next_type != 4 || status != 1 || is_loaded != 0 {
            State::get(env).hunters_pending_ship_load = None;
        } else if queued_at.elapsed() >= Duration::from_millis(1500) {
            State::get(env).hunters_pending_ship_load = None;
            let has_update: u8 = env.mem.read(ConstPtr::from_bits(ship_addr + 0xa0));
            let updates_singletons: u8 = env.mem.read(ConstPtr::from_bits(ship_addr + 0xa2));
            log!(
                "Hunters 2: stalled ship load; invoking ShipGameController.onLoad: has_update={has_update}, updates_singletons={updates_singletons}"
            );
            let _: () = msg![env; ship onLoad];
        }
    }

    // Deliver any queued stop/playback notifications before giving up the
    // last runtime retain. Their notification objects can be the same player.
    #[cfg(target_os = "android")]
    if std::mem::take(&mut State::get(env).hunters_replacement_scene_displayed) {
        let old_players = std::mem::take(&mut State::get(env).hunters_completed_player_holds);
        for player in old_players {
            log!("Hunters 2: releasing finished player after new CoreView displayed: {player:?}");
            release(env, player);
        }
    }
}

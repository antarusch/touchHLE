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
use crate::frameworks::core_graphics::{CGPoint, CGRect, CGSize};
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

// Restore the stacking order after Hunters 2's title movie ends or stops.
#[cfg(target_os = "android")]
fn restore_hunters_movie_overlay(env: &mut Environment, view: id) {
    if view == nil || env.bundle.bundle_identifier() != "uk.co.rodeogames.hunterstwo" {
        return;
    }
    let container: id = msg![env; view superview];
    if container == nil {
        return;
    }
    let parent: id = msg![env; container superview];
    if parent == nil {
        return;
    }
    () = msg![env; parent sendSubviewToBack:container];
    log!("Hunters 2 movie overlay: restored container {container:?} behind gameplay in {parent:?}");
}

#[cfg(target_os = "android")]
fn movie_video_tick(env: &mut Environment) {
    let Some(player) = State::get(env).active_player else {
        return;
    };
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
            // Hunters 2 puts its movie container behind an opaque game view.
            // Raise that container during playback, not the movie's subview:
            // the latter would still be covered by the game's full-screen view.
            if is_hunters_2 {
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
                        let parent: id = msg![env; container superview];
                        if parent != nil {
                            () = msg![env; parent bringSubviewToFront:container];
                            // Only the movie surface is noninteractive.
                            // Keep its container's save-slot controls enabled.
                            () = msg![env; view setUserInteractionEnabled:false];
                            log!(
                                "Hunters 2 movie overlay: raised container {container:?} in {parent:?}; video view {view:?} ignores touches"
                            );
                        }
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
        let pending = &mut State::get(env).pending_notifications;
        if !pending.iter().any(|(name, obj, _)| {
            *name == MPMoviePlayerPlaybackDidFinishNotification && *obj == player
        }) {
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
    #[cfg(target_os = "android")]
    let video_available = load_android_movie(env, this);
    #[cfg(not(target_os = "android"))]
    let video_available = false;
    if !video_available {
        log!("TODO: [(MPMoviePlayerController*){:?} play] - using simulated playback", this);
    }
    if let Some(old) = env.framework_state.media_player.movie_player.active_player {
        if old == this {
            let host = env.objc.borrow::<MPMoviePlayerControllerHostObject>(this);
            if host.playback_state == MPMoviePlaybackStatePlaying { return; }
            // Resuming a paused movie needs no additional runtime retain.
        } else {
            let _: () = msg![env; old stop];
        }
    }
    assert!(env.framework_state.media_player.movie_player.active_player.is_none());
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
        Some(host.view)
    } else {
        None
    };
    drop(host);
    #[cfg(target_os = "android")]
    if let Some(view) = restore_view {
        restore_hunters_movie_overlay(env, view);
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
                Some(host.view)
            } else {
                None
            };
            drop(host);
            #[cfg(target_os = "android")]
            if let Some(view) = restore_view {
                restore_hunters_movie_overlay(env, view);
            }
        }

        let name = ns_string::get_static_str(env, name_str);
        let center: id = msg_class![env; NSNotificationCenter defaultCenter];
        // TODO: should there be some user info attached?
        let _: () = msg![env; center postNotificationName:name object:object];

        // `play` takes one runtime retain while the player is active. Keep
        // that retain through the completion callback so the notification's
        // object stays valid, then release it after observers return.
        if release_active_player {
            release(env, object);
        }
    }
}

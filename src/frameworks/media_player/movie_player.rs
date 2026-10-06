/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `MPMoviePlayerController` etc.

use crate::dyld::{ConstantExports, HostConstant};
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
    /// Backing MPMoviePlayerController objects owned by MPMoviePlayerViewController.
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
    prepared: bool,
}
impl HostObject for MPMoviePlayerControllerHostObject {}

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
        prepared: false,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithContentURL:(id)url { // NSURL*
    log!(
        "TODO: [(MPMoviePlayerController*){:?} initWithContentURL:{:?} ({:?})]",
        this,
        url,
        ns_url::to_rust_path(env, url),
    );

    retain(env, url);

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

    let host = env
        .objc
        .borrow_mut::<MPMoviePlayerControllerHostObject>(this);
    host.content_url = url;
    host.view = view;

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
    todo_objc_setter!(this, initial_time);
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
    log!("TODO: [(MPMoviePlayerController*){:?} play]", this);
    if let Some(old) = env.framework_state.media_player.movie_player.active_player {
        let _: () = msg![env; old stop];
    }
    assert!(env.framework_state.media_player.movie_player.active_player.is_none());
    env.objc
        .borrow_mut::<MPMoviePlayerControllerHostObject>(this)
        .playback_state = MPMoviePlaybackStatePlaying;
    State::get(env).pending_notifications.push_back((
        MPMoviePlayerPlaybackStateDidChangeNotification,
        this,
        Instant::now(),
    ));
    // Movie player is retained by the runtime until it is stopped
    retain(env, this);
    env.framework_state.media_player.movie_player.active_player = Some(this);

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
    log!("TODO: [(MPMoviePlayerController*){:?} pause]", this);
    env.objc
        .borrow_mut::<MPMoviePlayerControllerHostObject>(this)
        .playback_state = MPMoviePlaybackStatePaused;
    State::get(env).pending_notifications.push_back((
        MPMoviePlayerPlaybackStateDidChangeNotification,
        this,
        Instant::now(),
    ));
}

- (())stop {
    log!("TODO: [(MPMoviePlayerController*){:?} stop]", this);
    env.objc
        .borrow_mut::<MPMoviePlayerControllerHostObject>(this)
        .playback_state = MPMoviePlaybackStateStopped;
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
    State::get(env)
        .view_controller_players
        .get(&this)
        .copied()
        .unwrap_or(nil)
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
            env.objc
                .borrow_mut::<MPMoviePlayerControllerHostObject>(object)
                .playback_state = MPMoviePlaybackStateStopped;
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

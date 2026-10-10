/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `CADisplayLink`

use crate::frameworks::foundation::ns_run_loop::NSRunLoopMode;
use crate::frameworks::foundation::ns_timer::set_time_interval;
use crate::frameworks::foundation::NSInteger;
use crate::objc::{
    autorelease, id, msg, msg_class, msg_send_no_type_checking, nil, objc_classes, release, retain,
    ClassExports, HostObject, NSZonePtr, SEL,
};

#[derive(Default)]
struct CADisplayLinkHostObject {
    target: id,
    selector: Option<SEL>,
    /// Weak reference. The timer retains the display link (as its target),
    /// so the timer necessarily outlives the display link. After `invalidate`,
    /// this pointer must not be used.
    ns_timer: id,
    paused: bool,
}
impl HostObject for CADisplayLinkHostObject {}

// Hunters 2 uses an asynchronous screen-unload handoff. In touchHLE the
// SaveMenuController sometimes remains active after the unload request.
// Complete the game's own callbacks only if it remains stuck for 90 frames.
#[cfg(target_os = "android")]
fn finish_stalled_hunters_intro_handoff(env: &mut crate::Environment, target: id) {
    use crate::mem::ConstPtr;
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

    static LAST_CONTROLLER: AtomicU32 = AtomicU32::new(0);
    static WAIT_FRAMES: AtomicUsize = AtomicUsize::new(0);

    if env.bundle.bundle_identifier() != "uk.co.rodeogames.hunterstwo" {
        return;
    }

    let game = target.to_bits();
    let status: i32 = env.mem.read(ConstPtr::from_bits(game + 0xc8));
    let next_type: i32 = env.mem.read(ConstPtr::from_bits(game + 0x9c));
    let current: u32 = env.mem.read(ConstPtr::from_bits(game + 0x98));
    let save_menu = if current != 0 {
        let controller_type: i32 = env.mem.read(ConstPtr::from_bits(current + 0xa4));
        controller_type == 1
    } else {
        false
    };

    if status != 3 || next_type != 6 || !save_menu {
        LAST_CONTROLLER.store(0, Ordering::Relaxed);
        WAIT_FRAMES.store(0, Ordering::Relaxed);
        return;
    }

    let frames = if LAST_CONTROLLER.load(Ordering::Relaxed) == current {
        WAIT_FRAMES.fetch_add(1, Ordering::Relaxed) + 1
    } else {
        LAST_CONTROLLER.store(current, Ordering::Relaxed);
        WAIT_FRAMES.store(1, Ordering::Relaxed);
        1
    };
    if frames != 90 {
        return;
    }

    // The game's SaveMenuController.onUnload directly calls the inherited
    // CoreViewController.onUnloadFinished, which notifies GameController and
    // releases the outgoing controller. Do not invoke onUnloadFinished again:
    // that would send a second completion to an already-released controller.
    let old_controller = id::from_bits(current);
    let on_unload = env.objc.lookup_selector("onUnload").unwrap();
    log!("Hunters 2 stalled unload: invoking SaveMenuController onUnload after {frames} frames");
    () = msg_send_no_type_checking(env, (old_controller, on_unload));
}

// Once the intro controller has been constructed, the normal fade-overlay
// notification (component 6) should advance GameController from status 1 to
// status 4. Hunters 2 may never deliver this event under touchHLE, leaving
// IntroTextController uninitialized. Recover only this exact stalled handoff.
#[cfg(target_os = "android")]
fn finish_stalled_hunters_intro_load(env: &mut crate::Environment, target: id) {
    use crate::mem::ConstPtr;
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

    static LAST_CONTROLLER: AtomicU32 = AtomicU32::new(0);
    static WAIT_FRAMES: AtomicUsize = AtomicUsize::new(0);

    if env.bundle.bundle_identifier() != "uk.co.rodeogames.hunterstwo" {
        return;
    }

    let game = target.to_bits();
    let status: i32 = env.mem.read(ConstPtr::from_bits(game + 0xc8));
    let current: u32 = env.mem.read(ConstPtr::from_bits(game + 0x98));
    if status != 1 || current == 0 {
        LAST_CONTROLLER.store(0, Ordering::Relaxed);
        WAIT_FRAMES.store(0, Ordering::Relaxed);
        return;
    }

    let controller_type: i32 = env.mem.read(ConstPtr::from_bits(current + 0xa4));
    let is_loaded: u8 = env.mem.read(ConstPtr::from_bits(current + 0xa3));
    if controller_type != 6 || is_loaded != 0 {
        LAST_CONTROLLER.store(0, Ordering::Relaxed);
        WAIT_FRAMES.store(0, Ordering::Relaxed);
        return;
    }

    let frames = if LAST_CONTROLLER.load(Ordering::Relaxed) == current {
        WAIT_FRAMES.fetch_add(1, Ordering::Relaxed) + 1
    } else {
        LAST_CONTROLLER.store(current, Ordering::Relaxed);
        WAIT_FRAMES.store(1, Ordering::Relaxed);
        1
    };
    if frames != 90 {
        return;
    }

    // This is the game's original overlay-show callback. Its status-1
    // branch calls setStatus:4 and the current controller's onInitialise,
    // which in turn calls IntroTextController.onLoad.
    log!("Hunters 2 stalled intro load: dispatching onComponentIsShowing:6 after {frames} frames");
    let on_showing = env.objc.lookup_selector("onComponentIsShowing:").unwrap();
    () = msg_send_no_type_checking(env, (target, on_showing, 6_i32));
}

// Defer Hunters 2 dialogue-overlay visibility probes until the next display
// tick. Querying guest UIView properties from within objc_msgSend is unsafe.
#[cfg(target_os = "android")]
static HUNTERS_DIALOGUE_CONTROLLER: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0);
#[cfg(target_os = "android")]
static HUNTERS_DIALOGUE_VISIBLE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
#[cfg(target_os = "android")]
static HUNTERS_DIALOGUE_FRAMES: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(target_os = "android")]
pub(crate) fn note_hunters_dialogue_overlay(controller: id, visible: bool) {
    use std::sync::atomic::Ordering;
    HUNTERS_DIALOGUE_CONTROLLER.store(controller.to_bits(), Ordering::Relaxed);
    HUNTERS_DIALOGUE_VISIBLE.store(visible, Ordering::Relaxed);
    HUNTERS_DIALOGUE_FRAMES.store(0, Ordering::Relaxed);
}

#[cfg(target_os = "android")]
fn probe_hunters_dialogue_overlay(env: &mut crate::Environment) {
    use std::sync::atomic::Ordering;
    if env.bundle.bundle_identifier_opt() != Some("uk.co.rodeogames.hunterstwo") {
        return;
    }
    let controller = HUNTERS_DIALOGUE_CONTROLLER.load(Ordering::Relaxed);
    if controller == 0 {
        return;
    }
    let frame = HUNTERS_DIALOGUE_FRAMES.fetch_add(1, Ordering::Relaxed) + 1;
    if frame != 1 && frame != 20 {
        return;
    }
    let visible = HUNTERS_DIALOGUE_VISIBLE.load(Ordering::Relaxed);
    let controller = id::from_bits(controller);
    let mut view: id = msg![env; controller view];
    let root = view;
    for depth in 0..6 {
        if view == nil {
            break;
        }
        let hidden: bool = msg![env; view isHidden];
        let alpha: f32 = msg![env; view alpha];
        let frame_rect: crate::frameworks::core_graphics::CGRect = msg![env; view frame];
        let layer: id = msg![env; view layer];
        let z: f32 = msg![env; layer zPosition];
        let subviews: id = msg![env; view subviews];
        let count: usize = msg![env; subviews count];
        log!(
            "Hunters 2 dialogue visibility: requested={visible}, frame={frame}, depth={depth}, view={view:?}, hidden={hidden}, alpha={alpha:.2}, z={z:.1}, rect={frame_rect:?}, children={count}"
        );
        view = msg![env; view superview];
    }
    if root != nil && frame == 20 {
        let children: id = msg![env; root subviews];
        let count: usize = msg![env; children count];
        for i in 0..count.min(8) {
            let child: id = msg![env; children objectAtIndex:i];
            let hidden: bool = msg![env; child isHidden];
            let alpha: f32 = msg![env; child alpha];
            let child_frame: crate::frameworks::core_graphics::CGRect =
                msg![env; child frame];
            let class = env.objc.try_get_class_name(msg![env; child class]).map(str::to_owned);
            log!(
                "Hunters 2 dialogue child: index={i}, view={child:?}, class={class:?}, hidden={hidden}, alpha={alpha:.2}, rect={child_frame:?}"
            );
        }
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation CADisplayLink: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    env.objc.alloc_object(this, Box::new(CADisplayLinkHostObject::default()), &mut env.mem)
}

+ (id)displayLinkWithTarget:(id)target selector:(SEL)sel {
    let display_link: id = msg![env; this new];
    // Because timer will pass itself as a second arg in ns_timer:handle_timer,
    // we need to use a re-direction: first fire the timer on the display link,
    // then call the original selector, passing the link as a second argument!
    let redirect_sel: SEL = env.objc.lookup_selector("_touchHLE_displayLinkTimerDidFire:").unwrap();
    let ns_timer = msg_class![env; NSTimer timerWithTimeInterval:(1.0/60.0)
                     target:display_link
                   selector:redirect_sel
                   userInfo:nil
                    repeats:true];
    retain(env, target);
    let host_object = env.objc.borrow_mut::<CADisplayLinkHostObject>(display_link);
    host_object.target = target;
    host_object.selector = Some(sel);
    host_object.ns_timer = ns_timer;
    log_dbg!("[CADisplayLink displayLinkWithTarget:{:?} selector:{}] => {:?}", target, sel.as_str(&env.mem), display_link);
    autorelease(env, display_link)
}

- (bool)isPaused {
    env.objc.borrow::<CADisplayLinkHostObject>(this).paused
}
- (())setPaused:(bool)paused {
    env.objc.borrow_mut::<CADisplayLinkHostObject>(this).paused = paused;
}

- (())setFrameInterval:(NSInteger)frameInterval {
    log_dbg!("[(CADisplayLink*){:?} setFrameInterval:{}]", this, frameInterval);
    assert!(frameInterval >= 1);
    let interval = frameInterval as f64 / 60.0;
    let ns_timer = env.objc.borrow::<CADisplayLinkHostObject>(this).ns_timer;
    set_time_interval(env, ns_timer, interval);
}

- (())addToRunLoop:(id)run_loop forMode:(NSRunLoopMode)mode {
    log_dbg!("[(CADisplayLink*){:?} addToRunLoop:{:?} forMode:{:?}]", this, run_loop, mode);
    let ns_timer = env.objc.borrow::<CADisplayLinkHostObject>(this).ns_timer;
    () = msg![env; run_loop addTimer:ns_timer forMode:mode];
}

- (())invalidate {
    log_dbg!("[(CADisplayLink*){:?} invalidate]", this);
    let ns_timer = env.objc.borrow::<CADisplayLinkHostObject>(this).ns_timer;
    () = msg![env; ns_timer invalidate];
}

- (())dealloc {
    let &CADisplayLinkHostObject { target, .. } = env.objc.borrow(this);
    release(env, target);
    env.objc.dealloc_object(this, &mut env.mem);
}

- (())_touchHLE_displayLinkTimerDidFire:(id)timer { // NSTimer *
    let &CADisplayLinkHostObject {
        target,
        selector,
        ns_timer,
        paused,
        ..
    } = env.objc.borrow::<CADisplayLinkHostObject>(this);
    assert_eq!(ns_timer, timer);
    if paused {
        // This could be improved, as we're still running the timer,
        // but just not passing the actual call.
        return;
    }
    let selector = selector.unwrap();
    log_once!("CADisplayLink callback fired");
    log!(
        "Diagnostic: CADisplayLink callback target={target:?}, selector={}",
        selector.as_str(&env.mem)
    );

    // Apple's documented callback takes the display link as an argument, but
    // some older apps use a zero-argument selector. Objective-C tolerates that,
    // so support both forms here.
    let selector_name = selector.as_str(&env.mem).to_owned();
    if selector_name.ends_with(':') {
        () = msg_send_no_type_checking(env, (target, selector, this));
    } else {
        () = msg_send_no_type_checking(env, (target, selector));
    }
    #[cfg(target_os = "android")]
    if selector_name == "gameUpdate" {
        finish_stalled_hunters_intro_handoff(env, target);
        finish_stalled_hunters_intro_load(env, target);
        probe_hunters_dialogue_overlay(env);
    }
}

@end

};

/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Functions, traits, and all kinds of things to assist with bridging the gap
//! between guest and host when it comes to animations in Core Animation.
//! Based in Apple's documented behavior for Core Animation, although not an
//! exact match.
//! References:
//! - Core Animation Programming Guide
//!   <https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/CoreAnimation_guide/Introduction/Introduction.html>
//! - List of Animatable properties
//!   <https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/CoreAnimation_guide/AnimatableProperties/AnimatableProperties.html#//apple_ref/doc/uid/TP40004514-CH11-SW2>
//! - Animation timing behavior, layers' local time, autoreverses, etc.
//!   <https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/CoreAnimation_guide/AdvancedAnimationTricks/AdvancedAnimationTricks.html>
//! - Algorithm for choosing interpolation values
//!   <https://developer.apple.com/documentation/quartzcore/cabasicanimation?language=objc>
use std::ops::Sub;

use crate::frameworks::core_animation::ca_animation::{
    get_animation_start_time, kCAFillModeBackwards, kCAFillModeBoth, kCAFillModeForwards,
    mark_completion,
};
use crate::frameworks::core_animation::ca_layer::remove_anonymous_animation;
use crate::frameworks::core_animation::{ca_layer::CALayerHostObject, CACurrentMediaTime};
use crate::frameworks::core_graphics::cg_color::CGColorHostObject;
use crate::frameworks::foundation::ns_string::{from_rust_string, to_rust_string};
use crate::objc::{id, msg, nil, release, retain};
use crate::Environment;

#[derive(Default)]
pub struct State {
    started_animations: Vec<id>,
    finished_animations: Vec<(id, id, bool, bool, Option<String>)>,
}

/// Local time within one animation, including delay, repetition and reversal.
fn sample_time(
    elapsed: f64,
    duration: f64,
    repeats: f32,
    repeat_duration: f64,
    autoreverses: bool,
) -> (f32, bool) {
    let duration = if duration > 0.0 { duration } else { 0.25 };
    let cycle = duration * if autoreverses { 2.0 } else { 1.0 };
    let count = if repeats > 0.0 {
        f64::from(repeats)
    } else {
        1.0
    };
    let limit = if repeat_duration > 0.0 {
        repeat_duration
    } else {
        cycle * count
    };
    let finished = elapsed >= limit;
    let elapsed = elapsed.max(0.0).min(limit);
    let mut within = elapsed % cycle;
    // At an integral completion, use the end of the last cycle.
    if finished && within == 0.0 && elapsed > 0.0 {
        within = cycle;
    }
    let progress = if autoreverses && within > duration {
        2.0 - within / duration
    } else {
        within / duration
    };
    (progress.clamp(0.0, 1.0) as f32, finished)
}

impl State {
    pub fn create_presentation_layer(
        &mut self,
        env: &mut Environment,
        layer: id,
    ) -> CALayerHostObject {
        let mut presentation = env.objc.borrow::<CALayerHostObject>(layer).clone();
        let mut animations: Vec<(Option<String>, id)> = presentation
            .animations
            .iter()
            .map(|(key, animation)| (Some(key.clone()), *animation))
            .collect();
        animations.extend(
            presentation
                .anonymous_animations
                .iter()
                .map(|animation| (None, *animation)),
        );
        let now = CACurrentMediaTime(env);
        for (key, animation) in animations {
            let begin: f64 = msg![env; animation beginTime];
            let start = get_animation_start_time(env, animation);
            if start.is_none() && now >= begin {
                // Nonzero beginTime is an absolute layer time. A zero value
                // starts at the first frame after the animation is added.
                *start = Some(if begin == 0.0 { now } else { begin });
                let effective_begin = start.unwrap_or(begin);
                retain(env, animation);
                self.started_animations.push(animation);
                self.animate_at(
                    env,
                    layer,
                    animation,
                    key,
                    now - effective_begin,
                    &mut presentation,
                );
                continue;
            }
            let effective_begin = start.unwrap_or(begin);
            self.animate_at(
                env,
                layer,
                animation,
                key,
                now - effective_begin,
                &mut presentation,
            );
        }
        presentation
    }

    fn animate_at(
        &mut self,
        env: &mut Environment,
        layer: id,
        animation: id,
        key: Option<String>,
        elapsed: f64,
        presentation: &mut CALayerHostObject,
    ) {
        let speed: f32 = msg![env; animation speed];
        let offset: f64 = msg![env; animation timeOffset];
        let elapsed = (elapsed) * f64::from(speed) + offset;
        let finished = Self::apply_animation(env, animation, elapsed, presentation, 0);
        if finished && mark_completion(env, animation) {
            let removed: bool = msg![env; animation isRemovedOnCompletion];
            retain(env, layer);
            retain(env, animation);
            self.finished_animations
                .push((layer, animation, true, removed, key));
        }
    }

    fn apply_animation(
        env: &mut Environment,
        animation: id,
        elapsed: f64,
        presentation: &mut CALayerHostObject,
        depth: usize,
    ) -> bool {
        assert!(depth < 64, "Cyclic animation group");
        let fill: id = msg![env; animation fillMode];
        let fill = to_rust_string(env, fill).to_string();
        let duration: f64 = msg![env; animation duration];
        let repeats: f32 = msg![env; animation repeatCount];
        let repeat_duration: f64 = msg![env; animation repeatDuration];
        let autoreverses: bool = msg![env; animation autoreverses];
        let (progress, finished) =
            sample_time(elapsed, duration, repeats, repeat_duration, autoreverses);
        if elapsed < 0.0 && fill != kCAFillModeBackwards && fill != kCAFillModeBoth {
            return false;
        }
        if finished && fill != kCAFillModeForwards && fill != kCAFillModeBoth {
            return true;
        }
        let timing: id = msg![env; animation timingFunction];
        let mut interpolation_amount: f32 = if timing == nil {
            progress
        } else {
            msg![env; timing _solveForInput:progress]
        };
        let class: crate::objc::Class = msg![env; animation class];
        let group = env.objc.get_known_class("CAAnimationGroup", &mut env.mem);
        if env.objc.class_is_subclass_of(class, group) {
            let children: id = msg![env; animation animations];
            let count: u32 = msg![env; children count];
            let local_time = f64::from(interpolation_amount) * duration;
            for i in 0..count {
                let child: id = msg![env; children objectAtIndex:i];
                let begin: f64 = msg![env; child beginTime];
                let speed: f32 = msg![env; child speed];
                let offset: f64 = msg![env; child timeOffset];
                Self::apply_animation(
                    env,
                    child,
                    (local_time - begin) * f64::from(speed) + offset,
                    presentation,
                    depth + 1,
                );
            }
            return finished;
        }
        let key_path: id = msg![env; animation keyPath];
        let keyframe = env
            .objc
            .get_known_class("CAKeyframeAnimation", &mut env.mem);
        let (from_value, to_value, by_value) = if env.objc.class_is_subclass_of(class, keyframe) {
            let values: id = msg![env; animation values];
            let count: u32 = msg![env; values count];
            if count == 0 {
                let path: id = msg![env; animation path];
                if path != nil {
                    let point = crate::frameworks::core_graphics::cg_path::point_at(
                        &env.objc,
                        path,
                        interpolation_amount,
                    );
                    if let Some(point) = point {
                        presentation.position = point;
                    }
                }
                return finished;
            }
            let times: id = msg![env; animation keyTimes];
            let time_count: u32 = msg![env; times count];
            let time_values: Vec<f32> = if time_count == count {
                (0..count)
                    .map(|i| {
                        let value: id = msg![env; times objectAtIndex:i];
                        msg![env; value floatValue]
                    })
                    .collect()
            } else if count > 1 {
                (0..count).map(|i| i as f32 / (count - 1) as f32).collect()
            } else {
                vec![0.0]
            };
            let (index, fraction) = keyframe_segment(&time_values, interpolation_amount);
            let mode: id = msg![env; animation calculationMode];
            let discrete = to_rust_string(env, mode) == "discrete";
            let end = if discrete {
                index
            } else {
                (index + 1).min(count as usize - 1)
            };
            interpolation_amount = if discrete { 0.0 } else { fraction };
            let functions: id = msg![env; animation timingFunctions];
            let functions_count: u32 = msg![env; functions count];
            if !discrete && (index as u32) < functions_count {
                let function: id = msg![env; functions objectAtIndex:(index as u32)];
                interpolation_amount = msg![env; function _solveForInput:interpolation_amount];
            }
            (
                msg![env; values objectAtIndex:(index as u32)],
                msg![env; values objectAtIndex:(end as u32)],
                nil,
            )
        } else {
            (
                msg![env; animation fromValue],
                msg![env; animation toValue],
                msg![env; animation byValue],
            )
        };
        // Update values only in the presentation layer
        let key_path = to_rust_string(env, key_path);
        // Only these properties are animatable
        // TODO: Implement for all properties
        match &*key_path {
            crate::frameworks::uikit::ui_view::TRANSITION_TIMING_KEY_PATH => {
                // UIView's transition fallback participates in the normal
                // animation lifecycle without changing layer properties.
            }
            "anchorPoint" => {
                let from_value = id_as_option(from_value).map(|obj| msg![env; obj CGPointValue]);
                let to_value = id_as_option(to_value).map(|obj| msg![env; obj CGPointValue]);
                let by_value = id_as_option(by_value).map(|obj| msg![env; obj CGPointValue]);
                let (from_value, by_value) = get_from_and_by_values(
                    Some(presentation.anchor_point),
                    from_value,
                    to_value,
                    by_value,
                );
                presentation.anchor_point = from_value + by_value * interpolation_amount;
            }
            "backgroundColor" => {
                let from_value =
                    id_as_option(from_value).map(|obj| *env.objc.borrow::<CGColorHostObject>(obj));
                let to_value =
                    id_as_option(to_value).map(|obj| *env.objc.borrow::<CGColorHostObject>(obj));
                let by_value =
                    id_as_option(by_value).map(|obj| *env.objc.borrow::<CGColorHostObject>(obj));
                let (from_value, by_value) = get_from_and_by_values(
                    presentation.background_color,
                    from_value,
                    to_value,
                    by_value,
                );
                presentation.background_color = Some(from_value + by_value * interpolation_amount)
            }
            "bounds" | "contentsCenter" => {
                let from_value = id_as_option(from_value).map(|obj| msg![env; obj CGRectValue]);
                let to_value = id_as_option(to_value).map(|obj| msg![env; obj CGRectValue]);
                let by_value = id_as_option(by_value).map(|obj| msg![env; obj CGRectValue]);
                let (from_value, by_value) = get_from_and_by_values(
                    Some(if &*key_path == "bounds" {
                        presentation.bounds
                    } else {
                        presentation.contents_center
                    }),
                    from_value,
                    to_value,
                    by_value,
                );
                let value = from_value + by_value * interpolation_amount;
                if &*key_path == "bounds" {
                    presentation.bounds = value;
                } else {
                    presentation.contents_center = value;
                }
            }
            "cornerRadius" => {
                let from_value = id_as_option(from_value).map(|obj| msg![env; obj floatValue]);
                let to_value = id_as_option(to_value).map(|obj| msg![env; obj floatValue]);
                let by_value = id_as_option(by_value).map(|obj| msg![env; obj floatValue]);
                let (from_value, by_value) = get_from_and_by_values(
                    Some(presentation.corner_radius),
                    from_value,
                    to_value,
                    by_value,
                );
                presentation.corner_radius = from_value + by_value * interpolation_amount;
            }
            "hidden" => {
                let from_value = id_as_option(from_value)
                    .map(|obj| msg![env; obj boolValue])
                    .map(|val: bool| val as i32 as f32);
                let to_value = id_as_option(to_value)
                    .map(|obj| msg![env; obj boolValue])
                    .map(|val: bool| val as i32 as f32);
                let by_value = id_as_option(by_value)
                    .map(|obj| msg![env; obj boolValue])
                    .map(|val: bool| val as i32 as f32);
                let (from_value, by_value) = get_from_and_by_values(
                    Some(presentation.hidden as i32 as f32),
                    from_value,
                    to_value,
                    by_value,
                );
                presentation.hidden = (from_value + by_value * interpolation_amount) > 0.5;
            }
            "contents" => {
                presentation.contents = if interpolation_amount < 1.0 {
                    if from_value == nil {
                        presentation.contents
                    } else {
                        from_value
                    }
                } else if to_value == nil {
                    presentation.contents
                } else {
                    to_value
                };
                presentation.gles_texture_is_up_to_date = false;
            }
            "transform" => {
                let from = id_as_option(from_value).map(|v| msg![env; v CATransform3DValue]);
                let to = id_as_option(to_value).map(|v| msg![env; v CATransform3DValue]);
                let by = id_as_option(by_value).map(|v| msg![env; v CATransform3DValue]);
                let (from, by) = get_from_and_by_values(Some(presentation.transform), from, to, by);
                presentation.transform = from + by * interpolation_amount;
                presentation.affine_transform = presentation.transform.affine();
            }
            "position.x" | "position.y" | "zPosition" | "borderWidth" => {
                let current = match &*key_path {
                    "position.x" => presentation.position.x,
                    "position.y" => presentation.position.y,
                    "zPosition" => presentation.z_position,
                    _ => presentation.border_width,
                };
                let from = id_as_option(from_value).map(|v| msg![env; v floatValue]);
                let to = id_as_option(to_value).map(|v| msg![env; v floatValue]);
                let by = id_as_option(by_value).map(|v| msg![env; v floatValue]);
                let (from, by) = get_from_and_by_values(Some(current), from, to, by);
                let value = from + by * interpolation_amount;
                match &*key_path {
                    "position.x" => presentation.position.x = value,
                    "position.y" => presentation.position.y = value,
                    "zPosition" => presentation.z_position = value,
                    _ => presentation.border_width = value,
                }
            }
            "opacity" => {
                let from_value = id_as_option(from_value).map(|obj| msg![env; obj floatValue]);
                let to_value = id_as_option(to_value).map(|obj| msg![env; obj floatValue]);
                let by_value = id_as_option(by_value).map(|obj| msg![env; obj floatValue]);
                let (from_value, by_value) = get_from_and_by_values(
                    Some(presentation.opacity),
                    from_value,
                    to_value,
                    by_value,
                );
                presentation.opacity = from_value + by_value * interpolation_amount;
            }
            "position" => {
                let from_value = id_as_option(from_value).map(|obj| msg![env; obj CGPointValue]);
                let to_value = id_as_option(to_value).map(|obj| msg![env; obj CGPointValue]);
                let by_value = id_as_option(by_value).map(|obj| msg![env; obj CGPointValue]);
                let (from_value, by_value) = get_from_and_by_values(
                    Some(presentation.position),
                    from_value,
                    to_value,
                    by_value,
                );
                presentation.position = from_value + by_value * interpolation_amount;
            }
            _ => panic!("Attempted to animate on key {}", key_path),
        }
        finished
    }

    pub fn update_started_and_finished_animations(self, env: &mut Environment) {
        for animation in self.started_animations {
            let delegate: id = msg![env; animation delegate];
            let selector = env
                .objc
                .register_host_selector("animationDidStart:".into(), &mut env.mem);
            if delegate != nil && msg![env; delegate respondsToSelector:selector] {
                () = msg![env; delegate animationDidStart:animation];
            }
            release(env, animation);
        }
        for (layer, animation, finished, removed, key) in self.finished_animations {
            let delegate: id = msg![env; animation delegate];
            let selector = env
                .objc
                .register_host_selector("animationDidStop:finished:".into(), &mut env.mem);
            // Remove before invoking guest code, which may add a replacement.
            if removed {
                if let Some(key) = key {
                    let current = env
                        .objc
                        .borrow::<CALayerHostObject>(layer)
                        .animations
                        .get(&key)
                        .copied();
                    if current == Some(animation) {
                        let key = from_rust_string(env, key);
                        () = msg![env; layer removeAnimationForKey:key];
                        release(env, key);
                    }
                } else {
                    remove_anonymous_animation(env, layer, animation);
                }
            }
            if delegate != nil && msg![env; delegate respondsToSelector:selector] {
                () = msg![env; delegate animationDidStop:animation finished:finished];
            }
            release(env, animation);
            release(env, layer);
        }
    }
}

fn keyframe_segment(times: &[f32], progress: f32) -> (usize, f32) {
    if times.len() <= 1 || progress <= times[0] {
        return (0, 0.0);
    }
    if progress >= times[times.len() - 1] {
        return (times.len() - 1, 0.0);
    }
    let index = times
        .windows(2)
        .position(|pair| progress < pair[1])
        .unwrap_or(times.len() - 2);
    let span = times[index + 1] - times[index];
    (
        index,
        if span > 0.0 {
            (progress - times[index]) / span
        } else {
            1.0
        },
    )
}

// Subtracting a value from itself supplies a generic zero delta.
#[allow(clippy::eq_op)]
fn get_from_and_by_values<T>(
    current_value: Option<T>,
    from_value: Option<T>,
    to_value: Option<T>,
    by_value: Option<T>,
) -> (T, T)
where
    T: Copy + Sub<Output = T>,
{
    if from_value.is_some() && to_value.is_some() && by_value.is_some() {
        panic!("Cannot specify all three of fromValue, toValue, and byValue");
    } else if let (Some(from_value), Some(to_value)) = (from_value, to_value) {
        let by_value = to_value - from_value.to_owned();
        (from_value, by_value)
    } else if let (Some(from_value), Some(by_value)) = (from_value, by_value) {
        (from_value.to_owned(), by_value.to_owned())
    } else if let (Some(to_value), Some(by_value)) = (to_value, by_value) {
        let from_value = to_value - by_value;
        (from_value, by_value.to_owned())
    } else if let Some(from_value) = from_value {
        let by_value = current_value.unwrap() - from_value;
        (from_value.to_owned(), by_value)
    } else if let Some(to_value) = to_value {
        let from_value = current_value.unwrap();
        let by_value = to_value - from_value;
        (from_value.to_owned(), by_value)
    } else if let Some(by_value) = by_value {
        let from_value = current_value.unwrap();
        (from_value.to_owned(), by_value.to_owned())
    } else {
        // TODO: All properties are nil. Interpolates between the previous
        // value of keyPath in the target layer’s presentation layer and the
        // current value of keyPath in the target layer’s presentation layer.
        let current = current_value.unwrap();
        (current, current - current)
    }
}

fn id_as_option(value: id) -> Option<id> {
    if value == nil {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
pub(crate) fn integration_check(env: &mut Environment) {
    check_animation_metadata_callback(env);
    use crate::frameworks::foundation::{ns_array, ns_string::get_static_str};
    use crate::objc::msg_class;
    let layer: id = msg_class![env; CALayer new];
    let key = get_static_str(env, "position.x");
    let animation: id = msg_class![env; CAKeyframeAnimation animationWithKeyPath:key];
    let from: id = msg_class![env; NSNumber numberWithFloat:10.0f32];
    let to: id = msg_class![env; NSNumber numberWithFloat:30.0f32];
    retain(env, from);
    retain(env, to);
    let values = ns_array::from_vec(env, vec![from, to]);
    () = msg![env; animation setValues:values];
    () = msg![env; animation setDuration:1.0f64];
    () = msg![env; animation setTimingFunction:nil];
    let group: id = msg_class![env; CAAnimationGroup animation];
    retain(env, animation);
    let children = ns_array::from_vec(env, vec![animation]);
    () = msg![env; group setAnimations:children];
    () = msg![env; group setDuration:1.0f64];
    () = msg![env; group setTimingFunction:nil];
    let mut presentation = env.objc.borrow::<CALayerHostObject>(layer).clone();
    assert!(!State::apply_animation(
        env,
        group,
        0.5,
        &mut presentation,
        0
    ));
    assert_eq!({ presentation.position.x }, 20.0);
    assert_eq!({ presentation.position.y }, 0.0);
    // The layer must own an independent animation copy.
    let group_key = get_static_str(env, "group");
    () = msg![env; layer addAnimation:group forKey:group_key];
    let stored = env.objc.borrow::<CALayerHostObject>(layer).animations["group"];
    assert_ne!(stored, group);
    () = msg![env; group setDuration:9.0f64];
    let duration: f64 = msg![env; stored duration];
    assert_eq!(duration, 1.0);
    let stored_children: id = msg![env; stored animations];
    let stored_child: id = msg![env; stored_children objectAtIndex:0u32];
    assert_ne!(stored_child, animation);
    () = msg![env; animation setDuration:7.0f64];
    let duration: f64 = msg![env; stored_child duration];
    assert_eq!(duration, 1.0);
    let mut state = State::default();
    state.animate_at(
        env,
        layer,
        stored,
        Some("group".into()),
        1.1,
        &mut presentation,
    );
    state.animate_at(
        env,
        layer,
        stored,
        Some("group".into()),
        1.2,
        &mut presentation,
    );
    assert_eq!(state.finished_animations.len(), 1);
    state.update_started_and_finished_animations(env);
    () = msg![env; layer removeAllAnimations];
    assert!(env
        .objc
        .borrow::<CALayerHostObject>(layer)
        .animations
        .is_empty());
    for object in [values, children, layer] {
        release(env, object);
    }
}

#[cfg(test)]
fn check_animation_metadata_callback(env: &mut Environment) {
    use crate::frameworks::foundation::ns_string::get_static_str;
    use crate::objc::msg_class;
    let layer: id = msg_class![env; CALayer new];
    let key_path = get_static_str(env, "position.x");
    let animation: id = msg_class![env; CABasicAnimation animationWithKeyPath:key_path];
    let name = get_static_str(env, "name");
    let level_key = get_static_str(env, "damageLevel");
    let value =
        crate::frameworks::foundation::ns_string::from_rust_string(env, "damageMovement".into());
    let level: id = msg_class![env; NSNumber numberWithInt:73i32];
    () = msg![env; animation setValue:value forKey:name];
    () = msg![env; animation setValue:level forKey:level_key];
    release(env, value);
    let target: id = msg_class![env; AnimationMetadataProbe new];
    () = msg![env; animation setDelegate:target];
    let end: id = msg_class![env; NSNumber numberWithFloat:10.0f32];
    () = msg![env; animation setToValue:end];
    () = msg![env; animation setDuration:1.0f64];
    let key = get_static_str(env, "damage");
    () = msg![env; layer addAnimation:animation forKey:key];
    let stored = env.objc.borrow::<CALayerHostObject>(layer).animations["damage"];
    () = msg![env; animation setValue:nil forKey:name];
    () = msg![env; animation setValue:nil forKey:level_key];
    let mut presentation = env.objc.borrow::<CALayerHostObject>(layer).clone();
    let mut state = State::default();
    for elapsed in [1.1, 1.2] {
        state.animate_at(
            env,
            layer,
            stored,
            Some("damage".into()),
            elapsed,
            &mut presentation,
        );
    }
    state.update_started_and_finished_animations(env);
    let count: i32 = msg![env; target tag];
    assert_eq!(count, 1);
    release(env, target);
    release(env, layer);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoints_reversal_and_repeat_duration() {
        assert_eq!(sample_time(1.0, 1.0, 0.0, 0.0, false), (1.0, true));
        assert_eq!(sample_time(1.5, 1.0, 0.0, 0.0, true), (0.5, false));
        assert_eq!(sample_time(2.0, 1.0, 0.0, 0.0, true), (0.0, true));
        assert_eq!(sample_time(2.5, 1.0, 3.0, 2.5, false), (0.5, true));
        assert_eq!(sample_time(-1.0, 1.0, 0.0, 0.0, false), (0.0, false));
    }
    #[test]
    fn keyframe_boundaries_and_duplicate_times() {
        let times = [0.0, 0.25, 0.75, 1.0];
        assert_eq!(keyframe_segment(&times, 0.5), (1, 0.5));
        assert_eq!(keyframe_segment(&times, 0.75), (2, 0.0));
        assert_eq!(keyframe_segment(&times, 1.0), (3, 0.0));
        assert_eq!(keyframe_segment(&[0.0, 0.5, 0.5, 1.0], 0.5), (2, 0.0));
    }
}

/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! The Media Player framework.

#[cfg(target_os = "android")]
mod android_video;
mod media_entity;
mod media_item_collection;
mod media_library;
mod media_picker_controller;
mod media_playlist;
mod media_query;
mod movie_player;
mod music_player;

pub const DYLIB: crate::dyld::HostDylib = crate::dyld::HostDylib {
    path: "/System/Library/Frameworks/MediaPlayer.framework/MediaPlayer",
    aliases: &[],
    class_exports: &[
        movie_player::CLASSES,
        music_player::CLASSES,
        media_entity::CLASSES,
        media_item_collection::CLASSES,
        media_library::CLASSES,
        media_picker_controller::CLASSES,
        media_playlist::CLASSES,
        media_query::CLASSES,
    ],
    constant_exports: &[movie_player::CONSTANTS, music_player::CONSTANTS],
    function_exports: &[],
};

#[derive(Default)]
pub struct State {
    movie_player: movie_player::State,
}

/// For use by `NSRunLoop`: check media players' status, send notifications if
/// necessary.
pub fn handle_players(env: &mut crate::Environment) {
    movie_player::handle_players(env);
}

#[cfg(target_os = "android")]
pub(crate) fn set_hunters_create_save_visible(
    env: &mut crate::Environment,
    visible: bool,
    controller: crate::objc::id,
) {
    movie_player::set_hunters_create_save_visible(env, visible, controller);
}

#[cfg(target_os = "android")]
pub(crate) fn set_hunters_message_visible(
    env: &mut crate::Environment,
    visible: bool,
    controller: crate::objc::id,
) {
    movie_player::set_hunters_message_visible(env, visible, controller);
}

#[cfg(target_os = "android")]
pub(crate) fn set_hunters_resume_contract_visible(
    env: &mut crate::Environment,
    visible: bool,
    controller: crate::objc::id,
) {
    movie_player::set_hunters_resume_contract_visible(env, visible, controller);
}

#[cfg(target_os = "android")]
pub(crate) fn set_hunters_gameplay_transition(env: &mut crate::Environment, started: bool) {
    movie_player::set_hunters_gameplay_transition(env, started);
}

#[cfg(target_os = "android")]
pub(crate) fn queue_hunters_save_menu_unload(
    env: &mut crate::Environment,
    game_controller: crate::objc::id,
    save_menu: crate::objc::id,
) {
    movie_player::queue_hunters_save_menu_unload(env, game_controller, save_menu);
}

#[cfg(target_os = "android")]
pub(crate) fn queue_hunters_ship_load(
    env: &mut crate::Environment,
    game: crate::objc::id,
    ship: crate::objc::id,
) {
    movie_player::queue_hunters_ship_load(env, game, ship);
}

#[cfg(target_os = "android")]
pub(crate) fn hunters_ship_load_started(env: &mut crate::Environment, ship: crate::objc::id) {
    movie_player::hunters_ship_load_started(env, ship);
}

#[cfg(target_os = "android")]
pub(crate) fn hunters_replacement_scene_displayed(env: &mut crate::Environment) {
    movie_player::hunters_replacement_scene_displayed(env);
}

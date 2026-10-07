/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `clocale.h`

use std::collections::hash_map::Entry;

use crate::dyld::FunctionExports;
use crate::environment::Environment;
use crate::export_c_func;
use crate::mem::{ConstPtr, MutPtr};

pub type LocaleCategory = i32;
pub const LC_ALL: LocaleCategory = 0;
pub const LC_COLLATE: LocaleCategory = 1;
pub const LC_CTYPE: LocaleCategory = 2;
pub const LC_MONETARY: LocaleCategory = 3;
pub const LC_NUMERIC: LocaleCategory = 4;
pub const LC_TIME: LocaleCategory = 5;
pub const LC_MESSAGES: LocaleCategory = 6;

#[derive(Default)]
pub struct State {
    locale: std::collections::HashMap<LocaleCategory, MutPtr<u8>>,
    langinfo: std::collections::HashMap<i32, MutPtr<u8>>,
}

pub fn setlocale(
    env: &mut Environment,
    category: LocaleCategory,
    locale: ConstPtr<u8>,
) -> MutPtr<u8> {
    assert!(matches!(
        category,
        LC_ALL | LC_COLLATE | LC_CTYPE | LC_MONETARY | LC_NUMERIC | LC_TIME | LC_MESSAGES
    ));
    if !locale.is_null() {
        // TODO: Handle empty locale string and ensure the combination of
        // category and locale is valid.
        let locale_cstr = env.mem.cstr_at(locale).to_owned();
        assert_ne!(locale_cstr.len(), 0);
        let new_locale = env.mem.alloc_and_write_cstr(locale_cstr.as_slice());
        if let Some(old_locale) = env.libc_state.clocale.locale.insert(category, new_locale) {
            env.mem.free(old_locale.cast())
        };

        // POSIX permits nl_langinfo() results to change after setlocale().
        // Discard cached guest strings so future queries reflect the new locale.
        let old_langinfo = std::mem::take(&mut env.libc_state.clocale.langinfo);
        for ptr in old_langinfo.into_values() {
            env.mem.free(ptr.cast());
        }
    } else if let Entry::Vacant(entry) = env.libc_state.clocale.locale.entry(category) {
        let default_locale = env.mem.alloc_and_write_cstr(b"C");
        entry.insert(default_locale);
    }
    env.libc_state.clocale.locale.get(&category).unwrap().cast()
}


fn codeset_for_current_locale(env: &Environment) -> Vec<u8> {
    let locale_ptr = env
        .libc_state
        .clocale
        .locale
        .get(&LC_CTYPE)
        .or_else(|| env.libc_state.clocale.locale.get(&LC_ALL));

    let Some(&locale_ptr) = locale_ptr else {
        return b"US-ASCII".to_vec();
    };

    let locale = env.mem.cstr_at(locale_ptr.cast());
    if locale == b"C" || locale == b"POSIX" {
        return b"US-ASCII".to_vec();
    }
    if locale == b"UTF-8" {
        return b"UTF-8".to_vec();
    }
    if let Some(dot) = locale.iter().position(|&byte| byte == b'.') {
        return locale[dot + 1..].to_vec();
    }

    // Darwin returns an empty string when the locale name has no explicit
    // codeset and is not one of the special C/POSIX/UTF-8 locale names.
    Vec::new()
}

fn c_langinfo(item: i32) -> &'static [u8] {
    const VALUES: [&[u8]; 58] = [
        b"US-ASCII",
        b"%a %b %e %H:%M:%S %Y",
        b"%m/%d/%y",
        b"%H:%M:%S",
        b"%I:%M:%S %p",
        b"AM",
        b"PM",
        b"Sunday",
        b"Monday",
        b"Tuesday",
        b"Wednesday",
        b"Thursday",
        b"Friday",
        b"Saturday",
        b"Sun",
        b"Mon",
        b"Tue",
        b"Wed",
        b"Thu",
        b"Fri",
        b"Sat",
        b"January",
        b"February",
        b"March",
        b"April",
        b"May",
        b"June",
        b"July",
        b"August",
        b"September",
        b"October",
        b"November",
        b"December",
        b"Jan",
        b"Feb",
        b"Mar",
        b"Apr",
        b"May",
        b"Jun",
        b"Jul",
        b"Aug",
        b"Sep",
        b"Oct",
        b"Nov",
        b"Dec",
        b"",
        b"",
        b"",
        b"",
        b"",
        b".",
        b"",
        b"^[yY]",
        b"^[nN]",
        b"yes",
        b"no",
        b"",
        b"md",
    ];

    usize::try_from(item)
        .ok()
        .and_then(|item| VALUES.get(item).copied())
        .unwrap_or(b"")
}

fn nl_langinfo(env: &mut Environment, item: i32) -> MutPtr<u8> {
    if let Some(&ptr) = env.libc_state.clocale.langinfo.get(&item) {
        return ptr;
    }

    let value = if item == 0 {
        codeset_for_current_locale(env)
    } else {
        c_langinfo(item).to_vec()
    };
    let ptr = env.mem.alloc_and_write_cstr(&value);
    env.libc_state.clocale.langinfo.insert(item, ptr);
    log_dbg!(
        "nl_langinfo({item}) => {:?}",
        env.mem.cstr_at_utf8(ptr.cast())
    );
    ptr
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(setlocale(_, _)),
    export_c_func!(nl_langinfo(_)),
];

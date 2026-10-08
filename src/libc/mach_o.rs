/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `Mach-O` related functions.

use crate::dyld::{export_c_func, FunctionExports};
use crate::mem::{ConstPtr, GuestUSize, MutPtr, Ptr};
use crate::Environment;

fn _NSGetExecutablePath(env: &mut Environment, buf: MutPtr<u8>, buf_size: MutPtr<u32>) -> i32 {
    let binding = env.bundle.executable_path();
    let bin_path = binding.as_str();

    let size = env.mem.read(buf_size);
    let len: GuestUSize = bin_path.len().try_into().unwrap();
    assert!(len < size);

    env.mem
        .bytes_at_mut(buf, len)
        .copy_from_slice(bin_path.as_bytes());
    env.mem.write(buf + len, b'\0');
    0
}

fn get_end(env: &mut Environment) -> u32 {
    // Assume app binary is the first.
    // From https://www.manpagez.com/man/3/get_end/
    // `In a Mach-O file <...> get_end returns the first address after
    // the last segment in the executable`
    // It was confirmed on a real device with the TestApp binary.
    env.bins[0].last_segment_end
}

fn _dyld_image_count(env: &mut Environment) -> u32 {
    env.bins.len().try_into().unwrap()
}

fn _dyld_get_image_header(env: &mut Environment, image_index: u32) -> ConstPtr<u8> {
    env.bins
        .get(image_index as usize)
        .map(|bin| Ptr::from_bits(bin.header_addr))
        .unwrap_or_else(Ptr::null)
}

fn _dyld_get_image_vmaddr_slide(env: &mut Environment, image_index: u32) -> i32 {
    env.bins
        .get(image_index as usize)
        .map(|bin| bin.vmaddr_slide as i32)
        .unwrap_or(0)
}

fn _dyld_get_image_name(env: &mut Environment, image_index: u32) -> ConstPtr<u8> {
    let Some(bin) = env.bins.get(image_index as usize) else {
        return Ptr::null();
    };

    // touchHLE currently has real guest Mach-O images only for the app and
    // bundled /usr/lib dylibs. Host-implemented frameworks have no guest
    // Mach-O header to enumerate.
    let path = if image_index == 0 {
        env.bundle.executable_path().as_str().to_owned()
    } else {
        format!("/usr/lib/{}", bin.name)
    };
    env.mem.alloc_and_write_cstr(path.as_bytes()).cast_const()
}

fn get_etext(env: &mut Environment) -> u32 {
    // Assume app binary is the first.
    let app_sections = &env.bins[0].sections;
    assert_eq!(
        app_sections
            .iter()
            .filter(|s| s.name.to_uppercase() == "__TEXT")
            .count(),
        1
    );
    let text_section = app_sections
        .iter()
        .find(|s| s.name.to_uppercase() == "__TEXT")
        .unwrap();
    text_section.next_section_addr()
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(_NSGetExecutablePath(_, _)),
    export_c_func!(_dyld_image_count()),
    export_c_func!(_dyld_get_image_header(_)),
    export_c_func!(_dyld_get_image_name(_)),
    export_c_func!(_dyld_get_image_vmaddr_slide(_)),
    export_c_func!(get_end()),
    export_c_func!(get_etext()),
];

/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `net/if.h`

use crate::dyld::FunctionExports;
use crate::export_c_func;
use crate::libc::errno::{set_errno, EINVAL, ENXIO};
use crate::mem::{ConstPtr, MutPtr, Ptr};
use crate::Environment;

const IF_NAMESIZE: usize = 16;

// touchHLE exposes a small deterministic set of guest interfaces rather than
// leaking host/Android interface details into the emulated iOS environment.
// These names are the conventional Darwin/iOS names used by applications.
const INTERFACES: &[(u32, &[u8])] = &[(1, b"lo0"), (2, b"en0"), (3, b"pdp_ip0")];

#[allow(non_camel_case_types)]
struct if_nameindex {
    _private: [u8; 0],
}

fn interface_name(index: u32) -> Option<&'static [u8]> {
    INTERFACES
        .iter()
        .find_map(|&(candidate, name)| (candidate == index).then_some(name))
}

fn interface_index(name: &[u8]) -> Option<u32> {
    INTERFACES
        .iter()
        .find_map(|&(index, candidate)| (candidate == name).then_some(index))
}

fn copy_interface_name(env: &mut Environment, name: &[u8], dest: MutPtr<u8>) {
    debug_assert!(name.len() < IF_NAMESIZE);
    for (offset, &byte) in name.iter().enumerate() {
        env.mem.write(dest + offset as u32, byte);
    }
    env.mem.write(dest + name.len() as u32, 0);
}

fn if_nametoindex(env: &mut Environment, ifname: ConstPtr<u8>) -> u32 {
    if ifname.is_null() {
        set_errno(env, EINVAL);
        return 0;
    }

    let name = env.mem.cstr_at(ifname);
    let Some(index) = interface_index(name) else {
        set_errno(env, ENXIO);
        return 0;
    };

    log_dbg!(
        "if_nametoindex({:?}) -> {}",
        String::from_utf8_lossy(name),
        index
    );
    index
}

fn if_indextoname(env: &mut Environment, ifindex: u32, ifname: MutPtr<u8>) -> MutPtr<u8> {
    if ifname.is_null() {
        set_errno(env, EINVAL);
        return Ptr::null();
    }

    let Some(name) = interface_name(ifindex) else {
        set_errno(env, ENXIO);
        return Ptr::null();
    };

    copy_interface_name(env, name, ifname);
    ifname
}

fn if_nameindex(env: &mut Environment) -> MutPtr<if_nameindex> {
    // Darwin's struct if_nameindex is two 32-bit fields on armv7:
    // unsigned int if_index; char *if_name;
    const ENTRY_SIZE: u32 = 8;
    let result: MutPtr<u8> = env
        .mem
        .alloc((INTERFACES.len() as u32 + 1) * ENTRY_SIZE)
        .cast();

    for (entry_index, &(if_index, name)) in INTERFACES.iter().enumerate() {
        let name_ptr: MutPtr<u8> = env.mem.alloc((name.len() + 1) as u32).cast();
        copy_interface_name(env, name, name_ptr);

        let entry = result + entry_index as u32 * ENTRY_SIZE;
        env.mem.write(entry.cast::<u32>(), if_index);
        env.mem.write((entry + 4).cast::<MutPtr<u8>>(), name_ptr);
    }

    let terminator = result + INTERFACES.len() as u32 * ENTRY_SIZE;
    env.mem.write(terminator.cast::<u32>(), 0);
    env.mem
        .write((terminator + 4).cast::<MutPtr<u8>>(), Ptr::null());

    result.cast()
}

fn if_freenameindex(env: &mut Environment, ptr: MutPtr<if_nameindex>) {
    if ptr.is_null() {
        return;
    }

    const ENTRY_SIZE: u32 = 8;
    let raw: MutPtr<u8> = ptr.cast();
    let mut offset = 0;
    loop {
        let entry = raw + offset;
        let index: u32 = env.mem.read(entry.cast());
        let name: MutPtr<u8> = env.mem.read((entry + 4).cast());
        if index == 0 && name.is_null() {
            break;
        }
        if !name.is_null() {
            env.mem.free(name.cast());
        }
        offset += ENTRY_SIZE;
    }

    env.mem.free(raw.cast());
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(if_nametoindex(_)),
    export_c_func!(if_indextoname(_, _)),
    export_c_func!(if_nameindex()),
    export_c_func!(if_freenameindex(_)),
];

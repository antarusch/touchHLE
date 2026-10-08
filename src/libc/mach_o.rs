/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `Mach-O` related functions.

use crate::dyld::{export_c_func, FunctionExports};
use crate::mem::{ConstPtr, GuestUSize, MutPtr, Ptr};
use crate::Environment;

const NX_LITTLE_ENDIAN: i32 = 1;

struct NXArchDefinition {
    name: &'static [u8],
    cputype: i32,
    cpusubtype: i32,
    description: &'static [u8],
}

const NX_ARCH_DEFINITIONS: &[NXArchDefinition] = &[
    NXArchDefinition {
        name: b"arm",
        cputype: mach_object::CPU_TYPE_ARM,
        cpusubtype: mach_object::CPU_SUBTYPE_ARM_ALL,
        description: b"ARM",
    },
    NXArchDefinition {
        name: b"armv4t",
        cputype: mach_object::CPU_TYPE_ARM,
        cpusubtype: mach_object::CPU_SUBTYPE_ARM_V4T,
        description: b"ARM v4T",
    },
    NXArchDefinition {
        name: b"armv5tej",
        cputype: mach_object::CPU_TYPE_ARM,
        cpusubtype: mach_object::CPU_SUBTYPE_ARM_V5TEJ,
        description: b"ARM v5TEJ",
    },
    NXArchDefinition {
        name: b"armv6",
        cputype: mach_object::CPU_TYPE_ARM,
        cpusubtype: mach_object::CPU_SUBTYPE_ARM_V6,
        description: b"ARM v6",
    },
    NXArchDefinition {
        name: b"armxscale",
        cputype: mach_object::CPU_TYPE_ARM,
        cpusubtype: mach_object::CPU_SUBTYPE_ARM_XSCALE,
        description: b"ARM XScale",
    },
    NXArchDefinition {
        name: b"armv7",
        cputype: mach_object::CPU_TYPE_ARM,
        cpusubtype: mach_object::CPU_SUBTYPE_ARM_V7,
        description: b"ARM v7",
    },
    NXArchDefinition {
        name: b"armv7f",
        cputype: mach_object::CPU_TYPE_ARM,
        cpusubtype: mach_object::CPU_SUBTYPE_ARM_V7F,
        description: b"ARM v7F",
    },
    NXArchDefinition {
        name: b"armv7s",
        cputype: mach_object::CPU_TYPE_ARM,
        cpusubtype: mach_object::CPU_SUBTYPE_ARM_V7S,
        description: b"ARM v7S",
    },
    NXArchDefinition {
        name: b"armv7k",
        cputype: mach_object::CPU_TYPE_ARM,
        cpusubtype: mach_object::CPU_SUBTYPE_ARM_V7K,
        description: b"ARM v7K",
    },
    NXArchDefinition {
        name: b"armv8",
        cputype: mach_object::CPU_TYPE_ARM,
        cpusubtype: mach_object::CPU_SUBTYPE_ARM_V8,
        description: b"ARM v8",
    },
];

fn nx_arch_definition_from_cpu_type(
    cputype: i32,
    cpusubtype: i32,
) -> Option<&'static NXArchDefinition> {
    let cpusubtype = cpusubtype & 0x00ff_ffff;
    NX_ARCH_DEFINITIONS
        .iter()
        .find(|arch| arch.cputype == cputype && arch.cpusubtype == cpusubtype)
        .or_else(|| {
            NX_ARCH_DEFINITIONS.iter().find(|arch| {
                arch.cputype == cputype && arch.cpusubtype == mach_object::CPU_SUBTYPE_ARM_ALL
            })
        })
}

fn allocate_nx_arch_info(env: &mut Environment, arch: &NXArchDefinition) -> ConstPtr<u8> {
    // struct NXArchInfo on 32-bit Darwin:
    // const char *name; cpu_type_t cputype; cpu_subtype_t cpusubtype;
    // enum NXByteOrder byteorder; const char *description;
    const NX_ARCH_INFO_SIZE: u32 = 20;

    let name = env.mem.alloc_and_write_cstr(arch.name).cast_const();
    let description = env
        .mem
        .alloc_and_write_cstr(arch.description)
        .cast_const();
    let info: MutPtr<u8> = env.mem.alloc(NX_ARCH_INFO_SIZE).cast();

    env.mem.write(info.cast::<ConstPtr<u8>>(), name);
    env.mem.write((info + 4).cast::<i32>(), arch.cputype);
    env.mem.write((info + 8).cast::<i32>(), arch.cpusubtype);
    env.mem.write((info + 12).cast::<i32>(), NX_LITTLE_ENDIAN);
    env.mem
        .write((info + 16).cast::<ConstPtr<u8>>(), description);

    info.cast_const()
}

fn NXGetArchInfoFromCpuType(
    env: &mut Environment,
    cputype: i32,
    cpusubtype: i32,
) -> ConstPtr<u8> {
    let Some(arch) = nx_arch_definition_from_cpu_type(cputype, cpusubtype) else {
        return Ptr::null();
    };
    log_dbg!(
        "NXGetArchInfoFromCpuType({}, {}) -> {:?}",
        cputype,
        cpusubtype,
        String::from_utf8_lossy(arch.name)
    );
    allocate_nx_arch_info(env, arch)
}

fn NXGetArchInfoFromName(env: &mut Environment, name: ConstPtr<u8>) -> ConstPtr<u8> {
    if name.is_null() {
        return Ptr::null();
    }

    let name = env.mem.cstr_at(name);
    let Some(arch) = NX_ARCH_DEFINITIONS.iter().find(|arch| arch.name == name) else {
        return Ptr::null();
    };
    allocate_nx_arch_info(env, arch)
}

fn NXGetLocalArchInfo(env: &mut Environment) -> ConstPtr<u8> {
    NXGetArchInfoFromCpuType(
        env,
        mach_object::CPU_TYPE_ARM,
        mach_object::CPU_SUBTYPE_ARM_V7,
    )
}

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
    export_c_func!(NXGetArchInfoFromCpuType(_, _)),
    export_c_func!(NXGetArchInfoFromName(_)),
    export_c_func!(NXGetLocalArchInfo()),
    export_c_func!(_NSGetExecutablePath(_, _)),
    export_c_func!(_dyld_image_count()),
    export_c_func!(_dyld_get_image_header(_)),
    export_c_func!(_dyld_get_image_name(_)),
    export_c_func!(_dyld_get_image_vmaddr_slide(_)),
    export_c_func!(get_end()),
    export_c_func!(get_etext()),
];

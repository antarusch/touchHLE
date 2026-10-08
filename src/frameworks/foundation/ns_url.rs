/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSURL`.

use super::ns_string::{from_rust_string, get_static_str, to_rust_string};
use super::NSUInteger;
use crate::fs::{GuestPath, GuestPathBuf};
use crate::mem::MutPtr;
use crate::objc::{
    autorelease, id, msg, msg_class, nil, objc_classes, release, retain, ClassExports, HostObject,
    NSZonePtr,
};
use crate::Environment;
use std::borrow::Cow;

/// It seems like there's two kinds of NSURLs: ones for file paths, and others.
/// So far only the former is implemented (TODO).
enum NSURLHostObject {
    /// This is a file URL. The NSString is a system path (no `file:///`).
    ///
    /// This is a wrapper around NSString so that conversions between NSURL
    /// and NSString, which happen often, can be simple and efficient.
    FileURL {
        ns_string: id,
        // Relative file URL save the working directory at the time of creation
        // At the moment, used in the description selector.
        working_directory: GuestPathBuf,
    },
    /// Non-file URL.
    OtherURL { ns_string: id },
}
impl HostObject for NSURLHostObject {}

fn has_url_scheme(url: &str) -> bool {
    let Some(colon) = url.find(':') else {
        return false;
    };
    if colon == 0 {
        return false;
    }
    let scheme = &url[..colon];
    scheme
        .chars()
        .enumerate()
        .all(|(idx, ch)| {
            if idx == 0 {
                ch.is_ascii_alphabetic()
            } else {
                ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.')
            }
        })
}

fn normalize_url_path(path: &str) -> String {
    let trailing_slash = path.ends_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }

    let mut result = String::from("/");
    result.push_str(&parts.join("/"));
    if trailing_slash && result != "/" {
        result.push('/');
    }
    result
}

fn resolve_relative_url(base: &str, relative: &str) -> String {
    if has_url_scheme(relative) {
        return relative.to_owned();
    }

    if let Some(scheme_end) = base.find("://") {
        let scheme = &base[..scheme_end];
        if relative.starts_with("//") {
            return format!("{scheme}:{relative}");
        }

        let after_scheme = &base[scheme_end + 3..];
        let authority_end = after_scheme
            .find(['/', '?', '#'])
            .unwrap_or(after_scheme.len());
        let authority = &after_scheme[..authority_end];
        let remainder = &after_scheme[authority_end..];

        if relative.starts_with('#') {
            let without_fragment = base.split('#').next().unwrap_or(base);
            return format!("{without_fragment}{relative}");
        }
        if relative.starts_with('?') {
            let without_query = base
                .split('#')
                .next()
                .unwrap_or(base)
                .split('?')
                .next()
                .unwrap_or(base);
            return format!("{without_query}{relative}");
        }

        let relative_path_end = relative
            .find(['?', '#'])
            .unwrap_or(relative.len());
        let relative_path = &relative[..relative_path_end];
        let relative_suffix = &relative[relative_path_end..];

        let path_and_more = remainder.split('#').next().unwrap_or(remainder);
        let base_path = path_and_more.split('?').next().unwrap_or(path_and_more);
        let combined_path = if relative_path.starts_with('/') {
            relative_path.to_owned()
        } else {
            let base_dir = match base_path.rfind('/') {
                Some(idx) => &base_path[..=idx],
                None => "/",
            };
            format!("{base_dir}{relative_path}")
        };

        return format!(
            "{scheme}://{authority}{}{}",
            normalize_url_path(&combined_path),
            relative_suffix
        );
    }

    if relative.starts_with('/') {
        return normalize_url_path(relative);
    }

    let base_without_suffix = base
        .split('#')
        .next()
        .unwrap_or(base)
        .split('?')
        .next()
        .unwrap_or(base);
    let base_dir = match base_without_suffix.rfind('/') {
        Some(idx) => &base_without_suffix[..=idx],
        None => "",
    };
    let combined = format!("{base_dir}{relative}");
    if combined.starts_with('/') {
        normalize_url_path(&combined)
    } else {
        combined
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSURL: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = NSURLHostObject::FileURL { ns_string: nil, working_directory: env.fs.working_directory().into() };
    env.objc.alloc_object(this, Box::new(host_object), &mut env.mem)
}

+ (id)URLWithString:(id)url { // NSString*
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithString:url];
    autorelease(env, new)
}

+ (id)URLWithString:(id)url relativeToURL:(id)base_url { // NSString*, NSURL*
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithString:url relativeToURL:base_url];
    autorelease(env, new)
}

+ (id)fileURLWithPath:(id)path { // NSString*
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initFileURLWithPath:path];
    autorelease(env, new)
}

+ (id)fileURLWithPath:(id)path // NSString*
          isDirectory:(bool)is_dir {
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initFileURLWithPath:path isDirectory:is_dir];
    autorelease(env, new)
}

- (())dealloc {
    match *env.objc.borrow(this) {
        NSURLHostObject::FileURL { ns_string, .. } => release(env, ns_string),
        NSURLHostObject::OtherURL { ns_string } => release(env, ns_string),
    }
    env.objc.dealloc_object(this, &mut env.mem)
}

// NSCopying implementation
- (id)copyWithZone:(NSZonePtr)_zone {
    retain(env, this)
}

- (id)initFileURLWithPath:(id)path { // NSString*
    // FIXME: this should guess whether the path is a directory
    msg![env; this initFileURLWithPath:path isDirectory:false]
}

- (id)initFileURLWithPath:(id)path // NSString*
              isDirectory:(bool)_is_dir {
    // FIXME: this does not resolve relative paths to be absolute!
    // TODO: this does not strip the file:/// prefix!
    assert!(!to_rust_string(env, path).starts_with("file:"));
    let path = msg![env; path stringByExpandingTildeInPath];
    let path: id = msg![env; path copy];
    *env.objc.borrow_mut(this) = NSURLHostObject::FileURL { ns_string: path, working_directory: env.fs.working_directory().into() };
    this
}

- (id)initWithString:(id)url { // NSString*
    if url == nil {
        return nil;
    }

    // FIXME: this should parse the URL
    assert!(!to_rust_string(env, url).starts_with("file:")); // TODO
    let url: id = msg![env; url copy];
    *env.objc.borrow_mut(this) = NSURLHostObject::OtherURL { ns_string: url };
    this
}

- (id)initWithString:(id)url relativeToURL:(id)base_url { // NSString*, NSURL*
    if url == nil {
        return nil;
    }
    if base_url == nil {
        return msg![env; this initWithString:url];
    }

    let relative = to_rust_string(env, url).into_owned();
    let base_string: id = msg![env; base_url absoluteString];
    let base = to_rust_string(env, base_string).into_owned();
    let resolved = resolve_relative_url(&base, &relative);
    let resolved = from_rust_string(env, resolved);
    let result: id = msg![env; this initWithString:resolved];
    release(env, resolved);
    result
}

- (bool)isFileURL {
    match env.objc.borrow(this) {
        NSURLHostObject::FileURL { .. } => true,
        NSURLHostObject::OtherURL { .. } => false,
    }
}

- (id)description {
    match env.objc.borrow(this) {
        NSURLHostObject::FileURL { ns_string, working_directory } => {
            let working_directory = working_directory.as_str().to_string();
            let mut description = to_rust_string(env, *ns_string).to_string().clone();
            if !description.starts_with('/') {
                description = format!("{} -- file://localhost{}", description.trim_start_matches("./"), working_directory );
            }
            let desc = from_rust_string(env, description);
            autorelease(env, desc)
        },
        NSURLHostObject::OtherURL { ns_string } => *ns_string,
    }
}

- (id)path {
    match *env.objc.borrow(this) {
        NSURLHostObject::FileURL { ns_string, .. } => ns_string,
        NSURLHostObject::OtherURL { ns_string } => {
            // TODO: Support full URLs, not only ones that are just a path.
            // FIXME: This should do unescaping.
            // TODO: Avoid copy.
            assert!(to_rust_string(env, ns_string).starts_with('/'));
            ns_string
        },
    }
}

- (id)absoluteString {
    match *env.objc.borrow(this) {
        // FIXME: file URLs should be rendered with a file:// scheme.
        NSURLHostObject::FileURL { ns_string, .. } => ns_string,
        // NSURL accepts arbitrary URL schemes (and relative URL strings),
        // not only HTTP(S). Without a base URL, absoluteString is the stored
        // string.
        NSURLHostObject::OtherURL { ns_string } => ns_string,
    }
}

- (id)absoluteURL {
    // FIXME: don't assume URL is already absolute
    let &NSURLHostObject::OtherURL { .. } = env.objc.borrow(this) else {
        unimplemented!(); // TODO
    };
    this
}

- (bool)getFileSystemRepresentation:(MutPtr<u8>)buffer
                          maxLength:(NSUInteger)buffer_size {
    let &NSURLHostObject::FileURL { ns_string, .. } = env.objc.borrow(this) else {
        unimplemented!(); // TODO
    };
    msg![env; ns_string getFileSystemRepresentation:buffer maxLength:buffer_size]
}

- (id)URLByAppendingPathComponent:(id)path_component // NSString *
                      isDirectory:(bool)is_directory {
    let &NSURLHostObject::FileURL { ns_string, .. } = env.objc.borrow(this) else {
        unimplemented!(); // TODO
    };
    let mut path: id = msg![env; ns_string stringByAppendingPathComponent:path_component];
    if is_directory {
        path = msg![env; path stringByAppendingString:(get_static_str(env, "/"))];
    }
    msg_class![env; NSURL fileURLWithPath:path]
}

- (id)URLByDeletingLastPathComponent {
    let &NSURLHostObject::FileURL { ns_string, .. } = env.objc.borrow(this) else {
        unimplemented!(); // TODO
    };
    let path: id = msg![env; ns_string stringByDeletingLastPathComponent];
    msg_class![env; NSURL fileURLWithPath:path]
}

// TODO: more constructors, more accessors

@end

// A caching layer a top of NSURL, it's OK to stub
// as we don't have yet a networking support
@implementation NSURLCache: NSObject
+ (id)sharedURLCache {
    // TODO
    nil
}
@end

};

/// Shortcut for host code, provides a view of a URL as a path.
/// TODO: Try to avoid allocating a new GuestPathBuf in more cases.
pub fn to_rust_path(env: &mut Environment, url: id) -> Cow<'static, GuestPath> {
    let path_string: id = msg![env; url path];

    match to_rust_string(env, path_string) {
        Cow::Borrowed(path) => Cow::Borrowed(path.as_ref()),
        Cow::Owned(path_buf) => Cow::Owned(path_buf.into()),
    }
}

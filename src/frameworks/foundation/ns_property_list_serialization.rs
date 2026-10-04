/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSPropertyListSerialization`.

use super::{ns_array, ns_data, ns_dictionary, ns_string, NSUInteger};
use super::{
    ns_array::ArrayHostObject, ns_dictionary::DictionaryHostObject, ns_value::NSNumberHostObject,
};
use crate::frameworks::core_foundation::time::apple_epoch;
use crate::frameworks::foundation::ns_date::NSDateHostObject;
use crate::fs::GuestPath;
use crate::mem::{MutPtr, MutVoidPtr};
use crate::objc::{
    autorelease, id, msg, msg_class, nil, objc_classes, release, Class, ClassExports,
};
use crate::Environment;
use plist::Value;
use std::collections::HashSet;
use std::io::Cursor;
use std::time::{Duration, SystemTime};

pub type NSPropertyListMutabilityOptions = NSUInteger;
pub const NSPropertyListImmutable: NSPropertyListMutabilityOptions = 0;
pub const NSPropertyListMutableContainers: NSPropertyListMutabilityOptions = 1;
pub const NSPropertyListMutableContainersAndLeaves: NSPropertyListMutabilityOptions = 2;

pub type NSPropertyListFormat = NSUInteger;
pub const NSPropertyListXMLFormat_v1_0: NSPropertyListFormat = 100;
pub const NSPropertyListBinaryFormat_v1_0: NSPropertyListFormat = 200;

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSPropertyListSerialization: NSObject

+ (id)dataFromPropertyList:(id)plist
                    format:(NSPropertyListFormat)format
                errorDescription:(MutPtr<id>)error_string { // NSString **
    let result = serialize_plist(env, plist, &mut HashSet::new()).and_then(|value| {
        let mut buf = Vec::new();
        let result = match format {
            NSPropertyListXMLFormat_v1_0 => value.to_writer_xml(&mut buf),
            NSPropertyListBinaryFormat_v1_0 => value.to_writer_binary(&mut buf),
            _ => return Err(format!("Unsupported property list format: {}", format)),
        };
        result.map_err(|error| error.to_string())?;
        Ok(buf)
    });
    let buf = match result {
        Ok(buf) => buf,
        Err(error) => {
            // The legacy errorDescription API returns an owned string, which
            // the caller must release.
            if !error_string.is_null() {
                let error = ns_string::from_rust_string(env, error);
                env.mem.write(error_string, error);
            }
            return nil;
        }
    };
    let len: u32 = buf.len().try_into().unwrap();
    log_dbg!("dataFromPropertyList buf len {}", len);
    let ptr = env.mem.alloc(len);
    env.mem.bytes_at_mut(ptr.cast(), len).copy_from_slice(&buf[..]);
    msg_class![env; NSData dataWithBytesNoCopy:ptr length:len]
}

+ (id)propertyListFromData:(id)data // NSData *
          mutabilityOption:(NSPropertyListMutabilityOptions)opt
                    format:(MutPtr<NSPropertyListFormat>)format
          errorDescription:(MutPtr<id>)error_string { // NSString **
    if data == nil {
        if !error_string.is_null() {
            let error_message = ns_string::from_rust_string(env, String::from("No plist data"));
            let error_message = autorelease(env, error_message);
            env.mem.write(error_string, error_message);
        }
        return nil;
    }
    let slice = ns_data::to_rust_slice(env, data);

    if let Ok(root) = Value::from_reader_xml(Cursor::new(slice)) {
        assert!(root.as_array().is_some() || root.as_dictionary().is_some());
        if !format.is_null() {
            env.mem.write(format, NSPropertyListXMLFormat_v1_0);
        }
        let property_list = deserialize_plist(env, &root, opt);
        return autorelease(env, property_list)
    }

    if let Ok(root) = Value::from_reader(Cursor::new(slice)) {
        assert!(root.as_array().is_some() || root.as_dictionary().is_some());
        if !format.is_null() {
            env.mem.write(format, NSPropertyListBinaryFormat_v1_0);
        }
        let property_list = deserialize_plist(env, &root, opt);
        return autorelease(env, property_list)
    }

    if !error_string.is_null() {
        let error_message = ns_string::from_rust_string(env, String::from("Failed to parse plist"));
        env.mem.write(error_string, error_message);
        autorelease(env, error_message);
    }

    nil
}

@end

};

/// Internals of `initWithContentsOfFile:` on `NSArray` and `NSDictionary`.
/// Returns `nil` on failure.
pub(super) fn deserialize_plist_from_file(
    env: &mut Environment,
    path: &GuestPath,
    array_expected: bool,
) -> id {
    log_dbg!("Reading plist from {:?}.", path);
    let Ok(bytes) = env.fs.read(path) else {
        log_dbg!("Couldn't read file, returning nil.");
        return nil;
    };

    let root = match Value::from_reader(Cursor::new(bytes)) {
        Ok(root) => root,
        Err(err) => {
            log_dbg!("Couldn't parse plist, returning nil: {}", err);
            return nil;
        }
    };

    if array_expected && root.as_array().is_none() {
        log_dbg!("Plist root is not array, returning nil.");
        return nil;
    }
    if !array_expected && root.as_dictionary().is_none() {
        log_dbg!("Plist root is not dictionary, returning nil.");
        return nil;
    }

    // Note: The top-most container mutability may change
    // depending on the caller.
    // (see `NSMutableArray` and `NSMutableDictionary` implementations)
    deserialize_plist(env, &root, NSPropertyListImmutable)
}

fn deserialize_plist(
    env: &mut Environment,
    value: &Value,
    mut_options: NSPropertyListMutabilityOptions,
) -> id {
    match value {
        Value::Array(array) => {
            let array = array
                .iter()
                .map(|value| deserialize_plist(env, value, mut_options))
                .collect();
            match mut_options {
                NSPropertyListImmutable => ns_array::from_vec(env, array),
                NSPropertyListMutableContainers | NSPropertyListMutableContainersAndLeaves => {
                    ns_array::mutable_from_vec(env, array)
                }
                _ => unreachable!(),
            }
        }
        Value::Dictionary(dict) => {
            let pairs: Vec<_> = dict
                .iter()
                .map(|(key, value)| {
                    (
                        ns_string::from_rust_string(env, key.clone()),
                        deserialize_plist(env, value, mut_options),
                    )
                })
                .collect();
            // Unlike ns_array::from_vec and ns_string::from_rust_string,
            // this will retain the keys and values!
            let ns_dict = match mut_options {
                NSPropertyListImmutable => ns_dictionary::dict_from_keys_and_objects(env, &pairs),
                NSPropertyListMutableContainers | NSPropertyListMutableContainersAndLeaves => {
                    ns_dictionary::mutable_dict_from_keys_and_objects(env, &pairs)
                }
                _ => unreachable!(),
            };
            // ...so they need to be released.
            for (key, value) in pairs {
                release(env, key);
                release(env, value);
            }
            ns_dict
        }
        Value::Boolean(b) => {
            let number: id = msg_class![env; NSNumber alloc];
            let b: bool = *b;
            msg![env; number initWithBool:b]
        }
        Value::Data(d) => {
            let length: NSUInteger = d.len().try_into().unwrap();
            let alloc: MutVoidPtr = env.mem.alloc(length);
            env.mem
                .bytes_at_mut(alloc.cast(), length)
                .copy_from_slice(d);
            let ns_data = match mut_options {
                NSPropertyListImmutable | NSPropertyListMutableContainers => {
                    msg_class![env; NSData alloc]
                }
                NSPropertyListMutableContainersAndLeaves => msg_class![env; NSMutableData alloc],
                _ => unreachable!(),
            };
            msg![env; ns_data initWithBytesNoCopy:alloc length:length]
        }
        Value::Date(date_val) => {
            let time: SystemTime = (*date_val).into();
            let time_interval = match time.duration_since(apple_epoch()) {
                Ok(interval) => interval.as_secs_f64(),
                Err(error) => -error.duration().as_secs_f64(),
            };
            let date: id = msg_class![env; NSDate alloc];
            msg![env; date initWithTimeIntervalSinceReferenceDate:time_interval]
        }
        Value::Integer(int) => {
            let number: id = msg_class![env; NSNumber alloc];
            // TODO: is this the correct order of preference? does it matter?
            if let Some(int64) = int.as_signed() {
                let longlong: i64 = int64;
                msg![env; number initWithLongLong:longlong]
            } else if let Some(uint64) = int.as_unsigned() {
                let ulonglong: u64 = uint64;
                msg![env; number initWithUnsignedLongLong:ulonglong]
            } else {
                unreachable!(); // according to plist crate docs
            }
        }
        Value::Real(real) => {
            let number: id = msg_class![env; NSNumber alloc];
            let double: f64 = *real;
            msg![env; number initWithDouble:double]
        }
        Value::String(s) => match mut_options {
            NSPropertyListImmutable | NSPropertyListMutableContainers => {
                ns_string::from_rust_string(env, s.clone())
            }
            NSPropertyListMutableContainersAndLeaves => {
                ns_string::mutable_from_rust_string(env, s.clone())
            }
            _ => unreachable!(),
        },
        Value::Uid(_) => {
            // These are probably only used by NSKeyedUnarchiver, which does not
            // currently use this code in our implementation.
            unimplemented!("deserialize plist value: {:?}", value);
        }
        _ => {
            unreachable!() // enum is marked inexhaustive, but shouldn't be
        }
    }
}

fn serialize_plist(
    env: &mut Environment,
    plist: id,
    active: &mut HashSet<id>,
) -> Result<Value, String> {
    if plist == nil {
        return Err("A property list cannot contain nil".into());
    }
    if !active.insert(plist) {
        return Err("A property list cannot contain a cycle".into());
    }
    let result = serialize_plist_value(env, plist, active);
    active.remove(&plist);
    result
}

fn serialize_plist_value(
    env: &mut Environment,
    plist: id,
    active: &mut HashSet<id>,
) -> Result<Value, String> {
    let class: Class = msg![env; plist class];
    let dict_class = env.objc.get_known_class("NSDictionary", &mut env.mem);
    let arr_class = env.objc.get_known_class("NSArray", &mut env.mem);
    let str_class = env.objc.get_known_class("NSString", &mut env.mem);
    let data_class = env.objc.get_known_class("NSData", &mut env.mem);

    if env.objc.class_is_subclass_of(class, dict_class) {
        // Snapshot references so recursive serialization never empties a
        // container, including when objects appear more than once.
        let key_vals: Vec<_> = env
            .objc
            .borrow::<DictionaryHostObject>(plist)
            .map
            .values()
            .flatten()
            .copied()
            .collect();
        let mut dict = plist::dictionary::Dictionary::new();
        for (key, val) in key_vals {
            let key_class: Class = msg![env; key class];
            if !env.objc.class_is_subclass_of(key_class, str_class) {
                return Err("Property list dictionary keys must be strings".into());
            }
            let key_string = ns_string::to_rust_string(env, key).to_string();
            dict.insert(key_string, serialize_plist(env, val, active)?);
        }
        Ok(Value::Dictionary(dict))
    } else if env.objc.class_is_subclass_of(class, arr_class) {
        let objects = env.objc.borrow::<ArrayHostObject>(plist).array.clone();
        let values = objects
            .into_iter()
            .map(|value| serialize_plist(env, value, active))
            .collect::<Result<_, _>>()?;
        Ok(Value::Array(values))
    } else if env.objc.class_is_subclass_of(class, str_class) {
        Ok(Value::String(
            ns_string::to_rust_string(env, plist).to_string(),
        ))
    } else if class == env.objc.get_known_class("NSNumber", &mut env.mem) {
        let num = env.objc.borrow::<NSNumberHostObject>(plist);
        Ok(match num {
            NSNumberHostObject::Bool(b) => Value::Boolean(*b),
            NSNumberHostObject::Int(i) => Value::from(*i),
            NSNumberHostObject::UnsignedInt(ui) => Value::from(*ui),
            NSNumberHostObject::Float(f) => Value::from(*f),
            NSNumberHostObject::Double(d) => Value::from(*d),
            NSNumberHostObject::LongLong(ll) => Value::from(*ll),
            NSNumberHostObject::UnsignedLongLong(ull) => Value::from(*ull),
            NSNumberHostObject::Short(s) => Value::from(*s),
            NSNumberHostObject::UnsignedShort(us) => Value::from(*us),
            NSNumberHostObject::Char(c) => Value::from(*c),
        })
    } else if env.objc.class_is_subclass_of(class, data_class) {
        Ok(Value::Data(ns_data::to_rust_slice(env, plist).to_vec()))
    } else if class == env.objc.get_known_class("NSDate", &mut env.mem) {
        let interval = env.objc.borrow::<NSDateHostObject>(plist).time_interval;
        let duration = Duration::try_from_secs_f64(interval.abs())
            .map_err(|_| "Invalid property list date".to_string())?;
        let time = if interval < 0.0 {
            apple_epoch().checked_sub(duration)
        } else {
            apple_epoch().checked_add(duration)
        }
        .ok_or_else(|| "Property list date is out of range".to_string())?;
        Ok(Value::Date(time.into()))
    } else {
        Err(format!(
            "Unsupported property list class: {}",
            env.objc.get_class_name(class)
        ))
    }
}

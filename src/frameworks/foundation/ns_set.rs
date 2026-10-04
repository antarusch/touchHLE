/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! The `NSSet` class cluster, including `NSMutableSet` and `NSCountedSet`.

use super::ns_array;
use super::ns_dictionary::DictionaryHostObject;
use super::ns_enumerator::{fast_enumeration_helper, NSFastEnumerationState};
use super::NSUInteger;
use crate::abi::DotDotDot;
use crate::environment::Environment;
use crate::mem::{ConstPtr, MutPtr};
use crate::objc::{
    autorelease, id, msg, msg_class, nil, objc_classes, release, retain, ClassExports, HostObject,
    NSZonePtr,
};

/// Belongs to _touchHLE_NSSet
#[derive(Debug, Default)]
struct SetHostObject {
    dict: DictionaryHostObject,
}
impl HostObject for SetHostObject {}

/// Retain one representative per distinct object, with a separate occurrence
/// count. Equality is Objective-C equality, not guest pointer equality.
#[derive(Default)]
struct CountedSetHostObject {
    objects: Vec<(id, NSUInteger)>,
}
impl HostObject for CountedSetHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

// NSSet is an abstract class. A subclass must provide:
// - (NSUInteger)count;
// - (id)member:(id)object;
// - (NSEnumerator*)objectEnumerator;
// We can pick whichever subclass we want for the various alloc methods.
// For the time being, that will always be _touchHLE_NSSet.
@implementation NSSet: NSObject

+ (id)allocWithZone:(NSZonePtr)zone {
    // NSSet might be subclassed by something which needs allocWithZone:
    // to have the normal behaviour. Unimplemented: call superclass alloc then.
    assert!(this == env.objc.get_known_class("NSSet", &mut env.mem));
    msg_class![env; _touchHLE_NSSet allocWithZone:zone]
}

+ (id)set {
    let set: id = msg![env; this new];
    autorelease(env, set)
}

+ (id)setWithArray:(id)array { // NSArray *
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithArray:array];
    autorelease(env, new)
}

+ (id)setWithObject:(id)object {
    assert!(object != nil);
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithObject:object];
    autorelease(env, new)
}

+ (id)setWithObjects:(id)first_obj, ...args {
    assert!(this == env.objc.get_known_class("NSSet", &mut env.mem));
    let new: id = msg![env; this alloc];
    env.objc.borrow_mut::<SetHostObject>(new).dict = set_from_objects(env, first_obj, args);
    autorelease(env, new)
}

// NSCopying implementation
- (id)copyWithZone:(NSZonePtr)_zone {
    retain(env, this)
}

- (bool)containsObject:(id)object {
    let enumerator: id = msg![env; this objectEnumerator];
    loop {
        let next: id = msg![env; enumerator nextObject];
        if next == nil {
            return false;
        }
        if msg![env; next isEqual:object] {
            return true;
        }
    }
}

@end

// NSMutableSet is an abstract class. A subclass must provide everything
// NSSet provides, plus:
// - (void)addObject:(id)object;
// - (void)removeObject:(id)object;
// Note that it inherits from NSSet, so we must ensure we override any default
// methods that would be inappropriate for mutability.
@implementation NSMutableSet: NSSet


+ (id)allocWithZone:(NSZonePtr)zone {
    // NSSet might be subclassed by something which needs allocWithZone:
    // to have the normal behaviour. Unimplemented: call superclass alloc then.
    assert!(this == env.objc.get_known_class("NSMutableSet", &mut env.mem));
    msg_class![env; _touchHLE_NSMutableSet allocWithZone:zone]
}

+ (id)setWithObjects:(id)first_obj, ...args {
    assert!(this == env.objc.get_known_class("NSMutableSet", &mut env.mem));
    let new: id = msg![env; this alloc];
    env.objc.borrow_mut::<SetHostObject>(new).dict = set_from_objects(env, first_obj, args);
    autorelease(env, new)
}

+ (id)setWithCapacity:(NSUInteger)capacity {
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithCapacity:capacity];
    autorelease(env, new)
}

- (())setSet:(id)other {
    if this == other {
        return;
    }
    let objects: id = msg![env; other allObjects];
    retain(env, objects);
    () = msg![env; this removeAllObjects];
    let count: NSUInteger = msg![env; objects count];
    for index in 0..count {
        let object: id = msg![env; objects objectAtIndex:index];
        () = msg![env; this addObject:object];
    }
    release(env, objects);
}


// NSCopying implementation
- (id)copyWithZone:(NSZonePtr)_zone {
    let objects: id = msg![env; this allObjects];
    let copy: id = msg_class![env; NSSet alloc];
    msg![env; copy initWithArray:objects]
}

@end

// Our private subclass that is the single implementation of NSSet for the
// time being.
@implementation _touchHLE_NSSet: NSSet

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(SetHostObject {
        dict: Default::default(),
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithObject:(id)object {
    let null: id = msg_class![env; NSNull null];

    let mut dict = <DictionaryHostObject as Default>::default();
    dict.insert(env, object, null, /* copy_key: */ false);

    env.objc.borrow_mut::<SetHostObject>(this).dict = dict;

    this
}

- (id)initWithObjects:(id)first_obj, ...args {
    env.objc.borrow_mut::<SetHostObject>(this).dict = set_from_objects(env, first_obj, args);
    this
}

- (id)initWithArray:(id)array {
    env.objc.borrow_mut::<SetHostObject>(this).dict = set_from_array(env, array);
    this
}

- (())dealloc {
    std::mem::take(&mut env.objc.borrow_mut::<SetHostObject>(this).dict).release(env);
    env.objc.dealloc_object(this, &mut env.mem)
}

// TODO: more init methods, etc

// TODO: accessors
- (NSUInteger)count {
    env.objc.borrow_mut::<SetHostObject>(this).dict.count
}

- (id)anyObject {
    let object_or_none = env.objc.borrow_mut::<SetHostObject>(this).dict.iter_keys().next();
    match object_or_none {
        Some(object) => object,
        None => nil
    }
}

- (id)allObjects {
    let objects = env.objc.borrow_mut::<SetHostObject>(this).dict.iter_keys().collect();
    ns_array::from_vec(env, objects)
}

- (id)objectEnumerator { // NSEnumerator*
    let array: id = msg![env; this allObjects];
    msg![env; array objectEnumerator]
}

// NSFastEnumeration implementation
- (NSUInteger)countByEnumeratingWithState:(MutPtr<NSFastEnumerationState>)state
                                  objects:(MutPtr<id>)stackbuf
                                    count:(NSUInteger)len {
    // We assume that order in which objects are reported is consistent
    // between calls!
    let objects: id = msg![env; this allObjects];
    let count: NSUInteger = msg![env; objects count];
    fast_enumeration_helper(env, this, |env, idx| {
        if idx < count {
            msg![env; objects objectAtIndex:idx]
        } else {
            nil
        }
    }, state, stackbuf, len)
}

@end

// Our private subclass that is the single implementation of NSMutableSet for
// the time being.
@implementation _touchHLE_NSMutableSet: NSMutableSet

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(SetHostObject {
        dict: Default::default(),
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithObject:(id)object {
    let null: id = msg_class![env; NSNull null];

    let mut dict = <DictionaryHostObject as Default>::default();
    dict.insert(env, object, null, /* copy_key: */ false);

    env.objc.borrow_mut::<SetHostObject>(this).dict = dict;

    this
}

- (id)initWithObjects:(id)first_obj, ...args {
    env.objc.borrow_mut::<SetHostObject>(this).dict = set_from_objects(env, first_obj, args);
    this
}

- (id)initWithArray:(id)array {
    env.objc.borrow_mut::<SetHostObject>(this).dict = set_from_array(env, array);
    this
}

- (id)initWithCapacity:(NSUInteger)_capacity {
    // TODO: capacity
    msg![env; this init]
}

- (())dealloc {
    std::mem::take(&mut env.objc.borrow_mut::<SetHostObject>(this).dict).release(env);
    env.objc.dealloc_object(this, &mut env.mem)
}

// TODO: init methods etc

- (NSUInteger)count {
    env.objc.borrow_mut::<SetHostObject>(this).dict.count
}

- (id)anyObject {
    let object_or_none = env.objc.borrow_mut::<SetHostObject>(this).dict.iter_keys().next();
    match object_or_none {
        Some(object) => object,
        None => nil
    }
}

- (id)allObjects {
    let objects = env.objc.borrow_mut::<SetHostObject>(this).dict.iter_keys().collect();
    ns_array::from_vec(env, objects)
}

- (id)objectEnumerator { // NSEnumerator*
    let array: id = msg![env; this allObjects];
    msg![env; array objectEnumerator]
}

// NSFastEnumeration implementation
- (NSUInteger)countByEnumeratingWithState:(MutPtr<NSFastEnumerationState>)state
                                  objects:(MutPtr<id>)stackbuf
                                    count:(NSUInteger)len {
    // TODO: check that set wasn't mutated!
    // We assume that order in which objects are reported is consistent
    // between calls!
    let objects: id = msg![env; this allObjects];
    let count: NSUInteger = msg![env; objects count];
    fast_enumeration_helper(env, this, |env, idx| {
        if idx < count {
            msg![env; objects objectAtIndex:idx]
        } else {
            nil
        }
    }, state, stackbuf, len)
}

// TODO: more mutation methods

- (())addObject:(id)object {
    let null: id = msg_class![env; NSNull null];
    let mut host_obj: SetHostObject = std::mem::take(env.objc.borrow_mut(this));
    host_obj.dict.insert(env, object, null, /* copy_key: */ false);
    *env.objc.borrow_mut(this) = host_obj;
}

- (())removeObject:(id)object {
    let mut host_obj: SetHostObject = std::mem::take(env.objc.borrow_mut(this));
    host_obj.dict.remove(env, object);
    *env.objc.borrow_mut(this) = host_obj;
}

- (())removeAllObjects {
    let mut old_host_obj = std::mem::replace(
        env.objc.borrow_mut(this),
        SetHostObject {
            dict: Default::default(),
        },
    );
    old_host_obj.dict.release(env);
}

- (())unionSet:(id)other { // NSSet *
    let enumerator: id = msg![env; other objectEnumerator];
    loop {
        let next: id = msg![env; enumerator nextObject];
        if next == nil {
            break;
        }
        () = msg![env; this addObject:next];
    }
}

@end

@implementation NSCountedSet: NSMutableSet

+ (id)allocWithZone:(NSZonePtr)_zone {
    env.objc.alloc_object(this, Box::<CountedSetHostObject>::default(), &mut env.mem)
}

+ (id)setWithObjects:(id)first_obj, ...args {
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new init];
    counted_set_add_objects(env, new, first_obj, args);
    autorelease(env, new)
}

+ (id)setWithObjects:(ConstPtr<id>)objects count:(NSUInteger)count {
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithObjects:objects count:count];
    autorelease(env, new)
}

+ (id)setWithSet:(id)set {
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithSet:set];
    autorelease(env, new)
}

- (id)initWithCapacity:(NSUInteger)capacity {
    env.objc.borrow_mut::<CountedSetHostObject>(this).objects.reserve(capacity as usize);
    this
}

- (id)initWithObject:(id)object {
    () = msg![env; this addObject:object];
    this
}

- (id)initWithObjects:(id)first_obj, ...args {
    counted_set_add_objects(env, this, first_obj, args);
    this
}

- (id)initWithObjects:(ConstPtr<id>)objects count:(NSUInteger)count {
    for i in 0..count {
        let object = env.mem.read(objects + i);
        () = msg![env; this addObject:object];
    }
    this
}

- (id)initWithArray:(id)array {
    let count: NSUInteger = msg![env; array count];
    for i in 0..count {
        let object: id = msg![env; array objectAtIndex:i];
        () = msg![env; this addObject:object];
    }
    this
}

- (id)initWithSet:(id)set {
    () = msg![env; this unionSet:set];
    this
}

- (NSUInteger)count {
    env.objc.borrow::<CountedSetHostObject>(this).objects.len().try_into().unwrap()
}

- (NSUInteger)countForObject:(id)object {
    match counted_set_member_index(env, this, object) {
        Some(idx) => env.objc.borrow::<CountedSetHostObject>(this).objects[idx].1,
        None => 0,
    }
}

- (id)member:(id)object {
    match counted_set_member_index(env, this, object) {
        Some(idx) => env.objc.borrow::<CountedSetHostObject>(this).objects[idx].0,
        None => nil,
    }
}

- (bool)containsObject:(id)object {
    counted_set_member_index(env, this, object).is_some()
}

- (id)anyObject {
    env.objc.borrow::<CountedSetHostObject>(this).objects.first().map_or(nil, |&(object, _)| object)
}

- (id)allObjects {
    let objects = env.objc.borrow::<CountedSetHostObject>(this)
        .objects.iter().map(|&(object, _)| object).collect::<Vec<_>>();
    let objects = objects.into_iter().map(|object| retain(env, object)).collect();
    let array = ns_array::from_vec(env, objects);
    autorelease(env, array)
}

- (id)objectEnumerator {
    let array: id = msg![env; this allObjects];
    msg![env; array objectEnumerator]
}

- (NSUInteger)countByEnumeratingWithState:(MutPtr<NSFastEnumerationState>)state
                                  objects:(MutPtr<id>)stackbuf
                                    count:(NSUInteger)len {
    fast_enumeration_helper(env, this, |env, idx| {
        env.objc.borrow::<CountedSetHostObject>(this).objects
            .get(idx as usize).map_or(nil, |&(object, _)| object)
    }, state, stackbuf, len)
}

- (())addObject:(id)object {
    assert!(object != nil);
    if let Some(idx) = counted_set_member_index(env, this, object) {
        let count = &mut env.objc.borrow_mut::<CountedSetHostObject>(this).objects[idx].1;
        *count = count.checked_add(1).unwrap();
    } else {
        let object = retain(env, object);
        env.objc.borrow_mut::<CountedSetHostObject>(this).objects.push((object, 1));
    }
}

- (())removeObject:(id)object {
    let Some(idx) = counted_set_member_index(env, this, object) else {
        return;
    };
    let objects = &mut env.objc.borrow_mut::<CountedSetHostObject>(this).objects;
    if objects[idx].1 > 1 {
        objects[idx].1 -= 1;
    } else {
        let (object, _) = objects.remove(idx);
        release(env, object);
    }
}

- (())removeAllObjects {
    let objects = std::mem::take(&mut env.objc.borrow_mut::<CountedSetHostObject>(this).objects);
    for (object, _) in objects {
        release(env, object);
    }
}

- (())unionSet:(id)other {
    let enumerator: id = msg![env; other objectEnumerator];
    loop {
        let next: id = msg![env; enumerator nextObject];
        if next == nil {
            break;
        }
        () = msg![env; this addObject:next];
    }
}

- (id)copyWithZone:(NSZonePtr)zone {
    counted_set_copy(env, this, zone)
}
- (id)mutableCopyWithZone:(NSZonePtr)zone {
    counted_set_copy(env, this, zone)
}

- (())dealloc {
    () = msg![env; this removeAllObjects];
    env.objc.dealloc_object(this, &mut env.mem)
}

@end

};

fn counted_set_member_index(env: &mut Environment, set: id, object: id) -> Option<usize> {
    let objects = env.objc.borrow::<CountedSetHostObject>(set).objects.clone();
    objects
        .iter()
        .position(|&(candidate, _)| candidate == object || msg![env; candidate isEqual:object])
}

fn counted_set_add_objects(env: &mut Environment, set: id, first_obj: id, args: DotDotDot) {
    if first_obj == nil {
        return;
    }
    () = msg![env; set addObject:first_obj];
    let mut varargs = args.start();
    loop {
        let next: id = varargs.next(env);
        if next == nil {
            break;
        }
        () = msg![env; set addObject:next];
    }
}

fn counted_set_copy(env: &mut Environment, set: id, zone: NSZonePtr) -> id {
    let objects = env.objc.borrow::<CountedSetHostObject>(set).objects.clone();
    let new: id = msg_class![env; NSCountedSet allocWithZone:zone];
    let new: id = msg![env; new init];
    let objects = objects
        .into_iter()
        .map(|(object, count)| (retain(env, object), count))
        .collect();
    env.objc.borrow_mut::<CountedSetHostObject>(new).objects = objects;
    new
}

/// Helper method shared between `initWithObjects:` of `_touchHLE_NSSet` and
/// `_touchHLE_NSMutableSet`
fn set_from_objects(env: &mut Environment, first_obj: id, args: DotDotDot) -> DictionaryHostObject {
    let null: id = msg_class![env; NSNull null];

    let mut dict = <DictionaryHostObject as Default>::default();
    dict.insert(env, first_obj, null, /* copy_key: */ false);
    let mut varargs = args.start();
    loop {
        let next_arg: id = varargs.next(env);
        if next_arg == nil {
            break;
        }
        dict.insert(env, next_arg, null, /* copy_key: */ false);
    }
    dict
}

/// Helper method shared between `initWithArray:` of `_touchHLE_NSSet` and
/// `_touchHLE_NSMutableSet`
fn set_from_array(env: &mut Environment, array: id) -> DictionaryHostObject {
    let null: id = msg_class![env; NSNull null];

    let mut dict = <DictionaryHostObject as Default>::default();
    let count: NSUInteger = msg![env; array count];
    for i in 0..count {
        let next: id = msg![env; array objectAtIndex:i];
        dict.insert(env, next, null, /* copy_key: */ false);
    }
    dict
}

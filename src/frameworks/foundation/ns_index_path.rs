/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Index paths, including UIKit's section and row category.
use crate::frameworks::foundation::{NSInteger, NSUInteger};
use crate::mem::{ConstPtr, MutPtr};
use crate::objc::{
    autorelease, id, msg, nil, objc_classes, retain, ClassExports, HostObject, NSZonePtr,
};
#[derive(Default)]
struct IndexPathHostObject {
    indexes: Vec<NSUInteger>,
}
impl HostObject for IndexPathHostObject {}
pub const CLASSES: ClassExports = objc_classes! {
(env, this, _cmd);
@implementation NSIndexPath: NSObject
+ (id)allocWithZone:(NSZonePtr)_zone {
    env.objc
        .alloc_object(this, Box::<IndexPathHostObject>::default(), &mut env.mem)
}
+ (id)indexPathForRow:(NSInteger)row inSection:(NSInteger)section {
    let object: id = msg![env; this new];
    env.objc.borrow_mut::<IndexPathHostObject>(object).indexes = vec![section as u32, row as u32];
    autorelease(env, object)
}
+ (id)indexPathWithIndex:(NSUInteger)index {
    let object: id = msg![env; this new];
    env.objc.borrow_mut::<IndexPathHostObject>(object).indexes = vec![index];
    autorelease(env, object)
}
+ (id)indexPathWithIndexes:(ConstPtr<NSUInteger>)indexes length:(NSUInteger)length {
    let object: id = msg![env; this alloc];
    let object: id = msg![env; object initWithIndexes:indexes length:length];
    autorelease(env, object)
}
- (id)initWithIndexes:(ConstPtr<NSUInteger>)indexes length:(NSUInteger)length {
    let indexes = (0..length).map(|i| env.mem.read(indexes + i)).collect();
    env.objc.borrow_mut::<IndexPathHostObject>(this).indexes = indexes;
    this
}
- (NSUInteger)length {
    env.objc.borrow::<IndexPathHostObject>(this).indexes.len() as u32
}
- (NSUInteger)indexAtPosition:(NSUInteger)position {
    env.objc.borrow::<IndexPathHostObject>(this).indexes[position as usize]
}
- (NSInteger)section {
    env.objc.borrow::<IndexPathHostObject>(this).indexes[0] as i32
}
- (NSInteger)row {
    env.objc.borrow::<IndexPathHostObject>(this).indexes[1] as i32
}
- (())getIndexes:(MutPtr<NSUInteger>)buffer {
    let indexes = env.objc.borrow::<IndexPathHostObject>(this).indexes.clone();
    for (i, value) in indexes.into_iter().enumerate() {
        env.mem.write(buffer + i as u32, value);
    }
}
- (id)copyWithZone:(NSZonePtr)_zone {
    retain(env, this)
}
- (bool)isEqual:(id)other {
    if other == nil {
        return false;
    }
    let class = env.objc.get_known_class("NSIndexPath", &mut env.mem);
    if !msg![env; other isKindOfClass:class] {
        return false;
    }
    env.objc.borrow::<IndexPathHostObject>(this).indexes
        == env.objc.borrow::<IndexPathHostObject>(other).indexes
}
- (NSUInteger)hash {
    env.objc
        .borrow::<IndexPathHostObject>(this)
        .indexes
        .iter()
        .fold(0u32, |hash, index| {
            hash.wrapping_mul(31).wrapping_add(*index)
        })
}
@end
};

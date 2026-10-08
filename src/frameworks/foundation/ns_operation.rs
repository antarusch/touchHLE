/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSOperation` and `NSOperationQueue`.
//!
//! This implements the core pre-GCD operation APIs used by older iOS apps.
//! Queue scheduling is intentionally conservative: operations are started
//! when added to a non-suspended queue. The queue retains operations for its
//! lifetime, matching the ownership guarantees callers rely on.

use std::time::Duration;

use super::{ns_array, NSInteger, NSUInteger};
use crate::objc::{
    autorelease, id, msg, nil, objc_classes, release, retain, ClassExports, HostObject, NSZonePtr,
};
use crate::Environment;

const NS_OPERATION_QUEUE_PRIORITY_NORMAL: NSInteger = 0;
const NS_OPERATION_QUEUE_DEFAULT_MAX_CONCURRENT_OPERATION_COUNT: NSInteger = -1;

struct NSOperationHostObject {
    cancelled: bool,
    executing: bool,
    finished: bool,
    queue_priority: NSInteger,
    dependencies: Vec<id>,
}
impl HostObject for NSOperationHostObject {}

struct QueuedOperation {
    operation: id,
    started: bool,
}

struct NSOperationQueueHostObject {
    operations: Vec<QueuedOperation>,
    suspended: bool,
    max_concurrent_operation_count: NSInteger,
    name: id,
}
impl HostObject for NSOperationQueueHostObject {}

fn operation_is_finished(env: &mut Environment, operation: id) -> bool {
    msg![env; operation isFinished]
}

fn start_queued_operations(env: &mut Environment, queue: id) {
    if env
        .objc
        .borrow::<NSOperationQueueHostObject>(queue)
        .suspended
    {
        return;
    }

    let to_start: Vec<id> = env
        .objc
        .borrow::<NSOperationQueueHostObject>(queue)
        .operations
        .iter()
        .filter(|entry| !entry.started)
        .map(|entry| entry.operation)
        .collect();

    for operation in to_start {
        if let Some(entry) = env
            .objc
            .borrow_mut::<NSOperationQueueHostObject>(queue)
            .operations
            .iter_mut()
            .find(|entry| entry.operation == operation)
        {
            entry.started = true;
        }
        () = msg![env; operation start];
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSOperation: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(NSOperationHostObject {
        cancelled: false,
        executing: false,
        finished: false,
        queue_priority: NS_OPERATION_QUEUE_PRIORITY_NORMAL,
        dependencies: Vec::new(),
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (())start {
    if env.objc.borrow::<NSOperationHostObject>(this).finished {
        return;
    }

    if env.objc.borrow::<NSOperationHostObject>(this).cancelled {
        env.objc.borrow_mut::<NSOperationHostObject>(this).finished = true;
        return;
    }

    env.objc.borrow_mut::<NSOperationHostObject>(this).executing = true;
    () = msg![env; this main];
    let host = env.objc.borrow_mut::<NSOperationHostObject>(this);
    host.executing = false;
    host.finished = true;
}

- (())main {
    // Default NSOperation does no work. Subclasses override this method.
}

- (())cancel {
    env.objc.borrow_mut::<NSOperationHostObject>(this).cancelled = true;
}

- (bool)isCancelled {
    env.objc.borrow::<NSOperationHostObject>(this).cancelled
}

- (bool)isExecuting {
    env.objc.borrow::<NSOperationHostObject>(this).executing
}

- (bool)isFinished {
    env.objc.borrow::<NSOperationHostObject>(this).finished
}

- (bool)isConcurrent {
    false
}

- (bool)isReady {
    if env.objc.borrow::<NSOperationHostObject>(this).executing
        || env.objc.borrow::<NSOperationHostObject>(this).finished
    {
        return false;
    }

    let dependencies = env
        .objc
        .borrow::<NSOperationHostObject>(this)
        .dependencies
        .clone();
    dependencies
        .into_iter()
        .all(|dependency| operation_is_finished(env, dependency))
}

- (())addDependency:(id)operation {
    retain(env, operation);
    env.objc
        .borrow_mut::<NSOperationHostObject>(this)
        .dependencies
        .push(operation);
}

- (())removeDependency:(id)operation {
    let dependencies = &mut env
        .objc
        .borrow_mut::<NSOperationHostObject>(this)
        .dependencies;
    if let Some(index) = dependencies.iter().position(|&item| item == operation) {
        dependencies.remove(index);
        release(env, operation);
    }
}

- (id)dependencies {
    let dependencies = env
        .objc
        .borrow::<NSOperationHostObject>(this)
        .dependencies
        .clone();
    for &dependency in &dependencies {
        retain(env, dependency);
    }
    let array = ns_array::from_vec(env, dependencies);
    autorelease(env, array)
}

- (NSInteger)queuePriority {
    env.objc.borrow::<NSOperationHostObject>(this).queue_priority
}

- (())setQueuePriority:(NSInteger)priority {
    env.objc
        .borrow_mut::<NSOperationHostObject>(this)
        .queue_priority = priority;
}

- (())waitUntilFinished {
    while !operation_is_finished(env, this) {
        env.sleep(Duration::from_millis(1));
    }
}

- (())dealloc {
    let dependencies =
        std::mem::take(&mut env.objc.borrow_mut::<NSOperationHostObject>(this).dependencies);
    for dependency in dependencies {
        release(env, dependency);
    }
    env.objc.dealloc_object(this, &mut env.mem)
}

@end

@implementation NSOperationQueue: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(NSOperationQueueHostObject {
        operations: Vec::new(),
        suspended: false,
        max_concurrent_operation_count:
            NS_OPERATION_QUEUE_DEFAULT_MAX_CONCURRENT_OPERATION_COUNT,
        name: nil,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (())addOperation:(id)operation {
    retain(env, operation);
    env.objc
        .borrow_mut::<NSOperationQueueHostObject>(this)
        .operations
        .push(QueuedOperation {
            operation,
            started: false,
        });
    start_queued_operations(env, this);
}

- (id)operations {
    let operations: Vec<id> = env
        .objc
        .borrow::<NSOperationQueueHostObject>(this)
        .operations
        .iter()
        .map(|entry| entry.operation)
        .collect();
    for &operation in &operations {
        retain(env, operation);
    }
    let array = ns_array::from_vec(env, operations);
    autorelease(env, array)
}

- (NSUInteger)operationCount {
    env.objc
        .borrow::<NSOperationQueueHostObject>(this)
        .operations
        .len()
        .try_into()
        .unwrap()
}

- (())setMaxConcurrentOperationCount:(NSInteger)count {
    env.objc
        .borrow_mut::<NSOperationQueueHostObject>(this)
        .max_concurrent_operation_count = count;
}

- (NSInteger)maxConcurrentOperationCount {
    env.objc
        .borrow::<NSOperationQueueHostObject>(this)
        .max_concurrent_operation_count
}

- (())setSuspended:(bool)suspended {
    let was_suspended = env
        .objc
        .borrow::<NSOperationQueueHostObject>(this)
        .suspended;
    env.objc
        .borrow_mut::<NSOperationQueueHostObject>(this)
        .suspended = suspended;
    if was_suspended && !suspended {
        start_queued_operations(env, this);
    }
}

- (bool)isSuspended {
    env.objc
        .borrow::<NSOperationQueueHostObject>(this)
        .suspended
}

- (())cancelAllOperations {
    let operations: Vec<id> = env
        .objc
        .borrow::<NSOperationQueueHostObject>(this)
        .operations
        .iter()
        .map(|entry| entry.operation)
        .collect();
    for operation in operations {
        () = msg![env; operation cancel];
    }
}

- (())waitUntilAllOperationsAreFinished {
    start_queued_operations(env, this);
    loop {
        let operations: Vec<id> = env
            .objc
            .borrow::<NSOperationQueueHostObject>(this)
            .operations
            .iter()
            .map(|entry| entry.operation)
            .collect();
        if operations
            .into_iter()
            .all(|operation| operation_is_finished(env, operation))
        {
            break;
        }
        env.sleep(Duration::from_millis(1));
    }
}

- (id)name {
    env.objc.borrow::<NSOperationQueueHostObject>(this).name
}

- (())setName:(id)name {
    retain(env, name);
    let old = std::mem::replace(
        &mut env.objc.borrow_mut::<NSOperationQueueHostObject>(this).name,
        name,
    );
    release(env, old);
}

- (())dealloc {
    let host = env.objc.borrow_mut::<NSOperationQueueHostObject>(this);
    let operations = std::mem::take(&mut host.operations);
    let name = std::mem::replace(&mut host.name, nil);
    for entry in operations {
        release(env, entry.operation);
    }
    release(env, name);
    env.objc.dealloc_object(this, &mut env.mem)
}

@end

};

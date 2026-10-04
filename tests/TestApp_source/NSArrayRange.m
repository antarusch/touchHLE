/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
#import "system_headers.h"
#include <stdio.h>

static BOOL checkArrayRange(BOOL condition, int line) {
  if (!condition)
    printf("NSArray range check failed at line %d\n", line);
  return condition;
}
#define CHECK(condition) ok &= checkArrayRange((condition), __LINE__)

int test_NSArray_subarray_ranges(void) {
  NSAutoreleasePool *pool = [NSAutoreleasePool new];
  NSObject *a = [NSObject new];
  NSObject *b = [NSObject new];
  NSObject *c = [NSObject new];
  NSObject *d = [NSObject new];
  NSObject *e = [NSObject new];
  NSMutableArray *source = [NSMutableArray arrayWithObjects:a, b, c, d, e, nil];
  // Eustrath selects four animation frames starting at index one.
  NSArray *slice = [source subarrayWithRange:NSMakeRange(1, 4)];
  BOOL ok = YES;
  CHECK([slice count] == 4);
  CHECK([slice objectAtIndex:0] == b);
  CHECK([slice objectAtIndex:1] == c);
  CHECK([slice objectAtIndex:2] == d);
  CHECK([slice objectAtIndex:3] == e);
  CHECK(![slice isKindOfClass:[NSMutableArray class]]);
  CHECK([source count] == 5);
  CHECK([[source subarrayWithRange:NSMakeRange(0, 0)] count] == 0);
  CHECK([[source subarrayWithRange:NSMakeRange(5, 0)] count] == 0);
  CHECK([[source subarrayWithRange:NSMakeRange(0, 5)] isEqualToArray:source]);
  CHECK([slice retainCount] == 1);
  [slice retain];

  NSArray *immutable = [NSArray arrayWithObjects:a, b, c, d, e, nil];
  CHECK([[immutable subarrayWithRange:NSMakeRange(1, 4)] isEqualToArray:slice]);
  CHECK([[immutable subarrayWithRange:NSMakeRange(5, 0)] count] == 0);
  CHECK([[[NSArray array] subarrayWithRange:NSMakeRange(0, 0)] count] == 0);
  [source removeAllObjects];
  CHECK([source count] == 0);
  CHECK([slice count] == 4);
  CHECK([slice objectAtIndex:0] == b);
  CHECK([slice objectAtIndex:3] == e);

  [pool drain];
  // The returned array is autoreleased and independently retains its elements.
  CHECK([slice retainCount] == 1);
  CHECK([a retainCount] == 1);
  CHECK([b retainCount] == 2);
  CHECK([c retainCount] == 2);
  CHECK([d retainCount] == 2);
  CHECK([e retainCount] == 2);
  [slice release];
  CHECK([b retainCount] == 1);
  CHECK([c retainCount] == 1);
  CHECK([d retainCount] == 1);
  CHECK([e retainCount] == 1);
  [a release];
  [b release];
  [c release];
  [d release];
  [e release];
  return ok ? 0 : -1;
}

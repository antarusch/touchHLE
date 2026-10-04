/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
#import "system_headers.h"
#include <pthread.h>

// Supplement the shared test headers with the missing declarations.
@interface CATransaction (ImplicitTransactionTest)
+ (void)flush;
@end

@interface NSNumber (ImplicitTransactionTest)
- (int)intValue;
@end

static void *transactionWorker(void *resultPointer) {
  NSAutoreleasePool *pool = [NSAutoreleasePool new];
  Class transaction =
      NSClassFromString([NSString stringWithUTF8String:"CATransaction"]);
  NSString *key = [NSString stringWithUTF8String:"ThreadTransactionProbe"];
  // No begin or layer mutation has occurred on this worker. Its first read
  // must return nil, even though the main thread holds a value for this key.
  BOOL ok = [transaction valueForKey:key] == nil;
  ok &= [transaction animationDuration] == 0.25;
  ok &= ![transaction disableActions];
  [transaction flush];
  [transaction setAnimationDuration:0.75];
  [transaction setDisableActions:YES];
  [transaction setValue:[NSNumber numberWithInt:42] forKey:key];
  ok &= [[transaction valueForKey:key] intValue] == 42;
  ok &= [transaction animationDuration] == 0.75;
  ok &= [transaction disableActions];
  [transaction flush];
  ok &= [transaction valueForKey:key] == nil;
  ok &= [transaction animationDuration] == 0.25;
  ok &= ![transaction disableActions];
  [transaction flush];
  *(int *)resultPointer = ok ? 0 : -1;
  [pool drain];
  return NULL;
}

int test_CATransaction_implicit_worker(void) {
  NSAutoreleasePool *pool = [NSAutoreleasePool new];
  Class transaction =
      NSClassFromString([NSString stringWithUTF8String:"CATransaction"]);
  NSString *key = [NSString stringWithUTF8String:"ThreadTransactionProbe"];
  [transaction flush];
  [transaction begin];
  [transaction setValue:[NSNumber numberWithInt:7] forKey:key];
  [transaction setAnimationDuration:2.0];
  [transaction setDisableActions:YES];
  pthread_t worker;
  int result = -2;
  if (pthread_create(&worker, NULL, transactionWorker, &result) != 0) {
    [transaction commit];
    [pool drain];
    return -3;
  }
  int joined = pthread_join(worker, NULL);
  BOOL ok = joined == 0 && result == 0;
  ok &= [[transaction valueForKey:key] intValue] == 7;
  ok &= [transaction animationDuration] == 2.0;
  ok &= [transaction disableActions];
  [transaction commit];
  [transaction flush];
  [pool drain];
  return ok ? 0 : -4;
}

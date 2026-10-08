/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
#import "system_headers.h"
#include <pthread.h>

@interface NSDictionary (NilDictionaryTest)
+ (instancetype)dictionaryWithDictionary:(NSDictionary *)dictionary;
+ (instancetype)dictionaryWithObject:(id)object forKey:(id)key;
- (instancetype)initWithDictionary:(NSDictionary *)dictionary;
@end

@interface NSMutableDictionary : NSDictionary
- (void)setObject:(id)object forKey:(id)key;
- (void)removeObjectForKey:(id)key;
- (void)setValue:(id)value forKey:(NSString *)key;
@end

static BOOL checkNilDictionary(void) {
  NSAutoreleasePool *pool = [NSAutoreleasePool new];
  NSDictionary *immutable = [NSDictionary dictionaryWithDictionary:nil];
  NSMutableDictionary *mutable =
      [NSMutableDictionary dictionaryWithDictionary:nil];
  NSDictionary *initialized = [[NSDictionary alloc] initWithDictionary:nil];
  NSMutableDictionary *initializedMutable =
      [[NSMutableDictionary alloc] initWithDictionary:nil];
  BOOL ok = immutable != nil && [immutable count] == 0 && mutable != nil &&
            [mutable count] == 0 && initialized != nil &&
            [initialized count] == 0 && initializedMutable != nil &&
            [initializedMutable count] == 0;
  NSString *key = [NSString stringWithUTF8String:"contents"];
  NSString *value = [NSString stringWithUTF8String:"action"];
  [mutable setObject:value forKey:key];
  NSDictionary *copy = [NSDictionary dictionaryWithDictionary:mutable];
  ok &= [mutable count] == 1 && [copy count] == 1 &&
        [copy objectForKey:key] == value;
  [mutable removeObjectForKey:key];
  ok &= [mutable count] == 0 && [copy count] == 1 &&
        [copy objectForKey:key] == value;

  // An inherited class factory must preserve NSMutableDictionary mutability.
  NSDictionary *singleImmutable =
      [NSDictionary dictionaryWithObject:value forKey:key];
  NSMutableDictionary *singleMutable =
      [NSMutableDictionary dictionaryWithObject:value forKey:key];
  NSString *additionalKey = [NSString stringWithUTF8String:"another"];
  [singleMutable setValue:value forKey:additionalKey];
  ok &= [singleMutable isKindOfClass:[NSMutableDictionary class]] &&
        [singleImmutable count] == 1 && [singleMutable count] == 2 &&
        [singleMutable objectForKey:additionalKey] == value;
  [initialized release];
  [initializedMutable release];
  [pool drain];
  return ok;
}

static void *dictionaryWorker(void *result) {
  *(BOOL *)result = checkNilDictionary();
  return NULL;
}

int test_NSDictionary_nil_source(void) {
  if (!checkNilDictionary()) {
    return -1;
  }
  BOOL result = NO;
  pthread_t worker;
  if (pthread_create(&worker, NULL, dictionaryWorker, &result) != 0) {
    return -2;
  }
  return pthread_join(worker, NULL) == 0 && result ? 0 : -3;
}

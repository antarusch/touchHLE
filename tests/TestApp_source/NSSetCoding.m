/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
#import "system_headers.h"

@interface NSData (SetCodingFixture)
- (BOOL)writeToFile:(NSString *)path atomically:(BOOL)atomically;
@end

int test_NSSet_coding(void) {
  NSAutoreleasePool *pool = [[NSAutoreleasePool alloc] init];
  NSString *alpha = [NSString stringWithUTF8String:"alpha"];
  NSString *beta = [NSString stringWithUTF8String:"beta"];
  NSArray *input = [NSArray arrayWithObjects:alpha, beta, alpha, nil];
  Class classes[] = {[NSSet class], [NSMutableSet class], [NSCountedSet class]};
  id roots[3];
  BOOL ok = YES;
  for (unsigned i = 0; i < 3; i++) {
    NSSet *set = [[classes[i] alloc] initWithArray:input];
    roots[i] = [NSArray arrayWithObjects:set, alpha, set, nil];
    NSData *data = [NSKeyedArchiver archivedDataWithRootObject:roots[i]];
    NSArray *decoded = [NSKeyedUnarchiver unarchiveObjectWithData:data];
    NSSet *restored = [decoded objectAtIndex:0];
    ok &= [restored isKindOfClass:classes[i]] && [restored count] == 2 &&
          [restored containsObject:alpha] && [restored containsObject:beta] &&
          restored == [decoded objectAtIndex:2];
    NSArray *members = [restored allObjects];
    BOOL shared = NO;
    for (NSUInteger j = 0; j < [members count]; j++) {
      shared |= [members objectAtIndex:j] == [decoded objectAtIndex:1];
    }
    ok &= shared;
    if (i == 2) {
      ok &= [(NSCountedSet *)restored countForObject:alpha] == 2 &&
            [(NSCountedSet *)restored countForObject:beta] == 1;
    } else if (i == 1) {
      [(NSMutableSet *)restored
          addObject:[NSString stringWithUTF8String:"gamma"]];
      ok &= [restored count] == 3 && [set count] == 2;
    }
    NSSet *empty =
        [[classes[i] alloc] initWithArray:[NSArray arrayWithObjects:nil]];
    data = [NSKeyedArchiver archivedDataWithRootObject:empty];
    restored = [NSKeyedUnarchiver unarchiveObjectWithData:data];
    ok &= [restored count] == 0 && [restored isKindOfClass:classes[i]];
    [empty release];
    [set release];
  }
#ifdef DEFINE_ME_WHEN_BUILDING_ON_MACOS
  // Preserve native Foundation bytes for an independent decoder check.
  NSArray *fixture =
      [NSArray arrayWithObjects:roots[0], roots[1], roots[2], nil];
  NSData *data = [NSKeyedArchiver archivedDataWithRootObject:fixture];
  ok &= [data writeToFile:[NSString stringWithUTF8String:"native_sets.bin"]
               atomically:NO];
#endif
  [pool drain];
  return ok ? 0 : 1;
}

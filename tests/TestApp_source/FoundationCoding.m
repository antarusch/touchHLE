/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
#import "system_headers.h"
#include <string.h>

@interface NSString (UTF8ValidationTest)
- (instancetype)initWithBytes:(const void *)bytes
                       length:(NSUInteger)length
                     encoding:(NSUInteger)encoding;
- (instancetype)initWithData:(NSData *)data encoding:(NSUInteger)encoding;
@end
@interface NSMutableString (ArchiveTest)
- (void)appendString:(NSString *)suffix;
@end
@interface NSData (ArchiveTest)
+ (instancetype)dataWithBytes:(const void *)bytes length:(NSUInteger)length;
- (const void *)bytes;
- (NSUInteger)length;
- (BOOL)writeToFile:(NSString *)path atomically:(BOOL)atomically;
@end
@interface NSMutableData : NSData
- (void)appendBytes:(const void *)bytes length:(NSUInteger)length;
@end
@interface NSDate : NSObject
- (instancetype)initWithTimeIntervalSinceReferenceDate:(NSTimeInterval)time;
- (NSTimeInterval)timeIntervalSinceReferenceDate;
@end
@interface NSNull : NSObject
+ (instancetype)null;
@end
@interface NSThread : NSObject
+ (NSThread *)mainThread;
+ (NSThread *)currentThread;
+ (BOOL)isMainThread;
- (BOOL)isMainThread;
@end

int test_Foundation_coding(void) {
  NSAutoreleasePool *pool = [NSAutoreleasePool new];
  NSString *text = [NSString stringWithUTF8String:"Café 火 😀"];
  NSMutableString *mutable = [[NSMutableString alloc] initWithString:text];
  unsigned char bytes[] = {0, 1, 0, 255};
  NSData *data = [NSData dataWithBytes:bytes length:sizeof(bytes)];
  NSMutableData *mutableData = [NSMutableData dataWithBytes:bytes
                                                     length:sizeof(bytes)];
  NSData *emptyData = [NSData dataWithBytes:NULL length:0];
  NSMutableData *emptyMutable = [NSMutableData dataWithBytes:NULL length:0];
  NSDate *date =
      [[NSDate alloc] initWithTimeIntervalSinceReferenceDate:1234.25];
  NSArray *root =
      [NSArray arrayWithObjects:mutable, text, mutable, data, mutableData, date,
                                [NSNull null], emptyData, emptyMutable, nil];
  NSData *archive = [NSKeyedArchiver archivedDataWithRootObject:root];
#ifdef DEFINE_ME_WHEN_BUILDING_ON_MACOS
  BOOL written = [archive
      writeToFile:[NSString stringWithUTF8String:"native_foundation.bin"]
       atomically:NO];
#else
  BOOL written = YES;
#endif
  NSArray *decoded = [NSKeyedUnarchiver unarchiveObjectWithData:archive];
  NSMutableString *restored = [decoded objectAtIndex:0];
  BOOL ok = written && [restored isKindOfClass:[NSMutableString class]] &&
            [restored isEqualToString:text] &&
            restored == [decoded objectAtIndex:2] &&
            [[decoded objectAtIndex:1] isEqualToString:text];
  [restored appendString:[NSString stringWithUTF8String:"!"]];
  ok &= [restored
            isEqualToString:[NSString stringWithUTF8String:"Café 火 😀!"]] &&
        [mutable isEqualToString:text] &&
        [[decoded objectAtIndex:1] isEqualToString:text];
  NSData *restoredData = [decoded objectAtIndex:3];
  NSMutableData *restoredMutable = [decoded objectAtIndex:4];
  ok &= [restoredData length] == sizeof(bytes) &&
        memcmp([restoredData bytes], bytes, sizeof(bytes)) == 0 &&
        [restoredMutable isKindOfClass:[NSMutableData class]] &&
        [restoredMutable length] == sizeof(bytes) &&
        memcmp([restoredMutable bytes], bytes, sizeof(bytes)) == 0;
  unsigned char extra = 7;
  [restoredMutable appendBytes:&extra length:1];
  ok &= [restoredMutable length] == 5 && [mutableData length] == 4 &&
        [(NSDate *)[decoded objectAtIndex:5] timeIntervalSinceReferenceDate] ==
            1234.25 &&
        [decoded objectAtIndex:6] == [NSNull null] &&
        [(NSData *)[decoded objectAtIndex:7] length] == 0 &&
        [(NSData *)[decoded objectAtIndex:8] length] == 0 &&
        [[decoded objectAtIndex:8] isKindOfClass:[NSMutableData class]];
  unsigned char bad[] = {0xdd, 0, 0, 0, 1};
  id badString = [[NSString alloc] initWithBytes:bad length:5 encoding:4];
  id badMutable = [[NSMutableString alloc] initWithBytes:bad
                                                  length:5
                                                encoding:4];
  NSData *badData = [NSData dataWithBytes:bad length:5];
  id badDataString = [[NSString alloc] initWithData:badData encoding:4];
  ok &= badString == nil && badMutable == nil && badDataString == nil;
  NSThread *main = [NSThread mainThread];
  ok &= main != nil && main == [NSThread mainThread] &&
        main == [NSThread currentThread] && [main isMainThread] &&
        [NSThread isMainThread];
  [date release];
  [mutable release];
  [pool drain];
  return ok ? 0 : 1;
}

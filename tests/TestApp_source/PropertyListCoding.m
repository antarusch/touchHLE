/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
#import "system_headers.h"
#include <string.h>

@interface NSPropertyListSerialization (PlistTest)
+ (NSData *)dataFromPropertyList:(id)list
                          format:(NSUInteger)format
                errorDescription:(NSString **)error;
@end
@interface NSData (PlistTest)
+ (instancetype)dataWithBytes:(const void *)bytes length:(NSUInteger)length;
- (const void *)bytes;
- (NSUInteger)length;
- (BOOL)writeToFile:(NSString *)path atomically:(BOOL)atomically;
@end
@interface NSMutableData : NSData
@end
@interface NSDate : NSObject
- (instancetype)initWithTimeIntervalSinceReferenceDate:(NSTimeInterval)time;
- (NSTimeInterval)timeIntervalSinceReferenceDate;
@end
@interface NSNumber (PlistTest)
+ (NSNumber *)numberWithDouble:(double)value;
- (double)doubleValue;
- (int)intValue;
- (BOOL)boolValue;
@end

int test_PropertyList_coding(void) {
  NSAutoreleasePool *pool = [NSAutoreleasePool new];
  NSString *text = [NSString stringWithUTF8String:"Café 火 😀 <&>"];
  unsigned char bytes[] = {0, 1, 0, 255};
  NSData *data = [NSMutableData dataWithBytes:bytes length:sizeof(bytes)];
  NSData *empty = [NSMutableData dataWithBytes:NULL length:0];
  NSDate *date =
      [[NSDate alloc] initWithTimeIntervalSinceReferenceDate:-31622400.0];
  NSArray *items =
      [NSArray arrayWithObjects:[NSString stringWithUTF8String:"one"],
                                [NSString stringWithUTF8String:"two"], nil];
  NSArray *keys =
      [NSArray arrayWithObjects:[NSString stringWithUTF8String:"text"],
                                [NSString stringWithUTF8String:"seen"],
                                [NSString stringWithUTF8String:"integer"],
                                [NSString stringWithUTF8String:"real"],
                                [NSString stringWithUTF8String:"bytes"],
                                [NSString stringWithUTF8String:"empty"],
                                [NSString stringWithUTF8String:"date"],
                                [NSString stringWithUTF8String:"items"], nil];
  NSArray *values =
      [NSArray arrayWithObjects:text, [NSNumber numberWithBool:YES],
                                [NSNumber numberWithInt:-42],
                                [NSNumber numberWithDouble:0.625], data, empty,
                                date, items, nil];
  NSDictionary *root = [NSDictionary dictionaryWithObjects:values forKeys:keys];
  BOOL ok = YES;
  for (NSUInteger format = 100; format <= 200; format += 100) {
    NSString *error = nil;
    NSData *encoded = [NSPropertyListSerialization dataFromPropertyList:root
                                                                 format:format
                                                       errorDescription:&error];
    ok &= encoded != nil;
#ifdef DEFINE_ME_WHEN_BUILDING_ON_MACOS
    NSString *path =
        [NSString stringWithUTF8String:format == 100 ? "native_plist.xml"
                                                     : "native_plist.bin"];
    ok &= [encoded writeToFile:path atomically:NO];
#endif
    NSUInteger detected = 0;
    NSDictionary *decoded =
        [NSPropertyListSerialization propertyListFromData:encoded
                                         mutabilityOption:2
                                                   format:&detected
                                         errorDescription:&error];
    ok &=
        decoded != nil && detected == format && [decoded count] == 8 &&
        [[decoded objectForKey:[NSString stringWithUTF8String:"text"]]
            isEqualToString:text] &&
        [[decoded objectForKey:[NSString stringWithUTF8String:"items"]]
            isEqualToArray:items] &&
        [(NSNumber *)[decoded
            objectForKey:[NSString stringWithUTF8String:"seen"]] boolValue] &&
        [(NSNumber *)[decoded
            objectForKey:[NSString stringWithUTF8String:"integer"]] intValue] ==
            -42 &&
        [(NSNumber *)[decoded
            objectForKey:[NSString stringWithUTF8String:"real"]] doubleValue] ==
            0.625 &&
        [(NSData *)[decoded
            objectForKey:[NSString stringWithUTF8String:"empty"]] length] == 0;
    NSData *restored =
        [decoded objectForKey:[NSString stringWithUTF8String:"bytes"]];
    ok &= [restored isKindOfClass:[NSMutableData class]] &&
          [restored length] == sizeof(bytes) &&
          memcmp([restored bytes], bytes, sizeof(bytes)) == 0;
    NSDate *restoredDate =
        [decoded objectForKey:[NSString stringWithUTF8String:"date"]];
    ok &= [restoredDate timeIntervalSinceReferenceDate] == -31622400.0;
  }
  [date release];
  [pool drain];
  return ok ? 0 : 1;
}

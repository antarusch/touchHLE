/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
#import "system_headers.h"
#include <stdio.h>

static BOOL sameStretchRect(CGRect a, CGRect b) {
  return a.origin.x == b.origin.x && a.origin.y == b.origin.y &&
         a.size.width == b.size.width && a.size.height == b.size.height;
}
static BOOL checkContentStretch(BOOL condition, int line) {
  if (!condition)
    printf("Content stretch check failed at line %d\n", line);
  return condition;
}
#define CHECK(condition) ok &= checkContentStretch((condition), __LINE__)

int test_UIView_contentStretch(void) {
  NSAutoreleasePool *pool = [NSAutoreleasePool new];
  CGRect unit = CGRectMake(0, 0, 1, 1);
  CGRect center = CGRectMake(0.25, 0.25, 0.5, 0.5);
  UIView *plain = [[UIView alloc] initWithFrame:CGRectMake(0, 0, 20, 30)];
  BOOL ok = YES;
  CHECK(sameStretchRect([plain contentStretch], unit));
  CHECK(sameStretchRect([[plain layer] contentsCenter], unit));
  [plain setContentStretch:center];
  CHECK(sameStretchRect([plain contentStretch], center));
  CHECK(sameStretchRect([[plain layer] contentsCenter], center));
  [[plain layer] setContentsCenter:unit];
  CHECK(sameStretchRect([plain contentStretch], unit));
  [plain release];

  CGColorSpaceRef cs = CGColorSpaceCreateDeviceRGB();
  CGContextRef context = CGBitmapContextCreate(NULL, 8, 8, 8, 32, cs,
                                               kCGImageAlphaPremultipliedLast);
  CGImageRef cgImage = CGBitmapContextCreateImage(context);
  UIImage *image = [UIImage imageWithCGImage:cgImage];
  CGImageRelease(cgImage);
  CGContextRelease(context);
  CGColorSpaceRelease(cs);
  UIImageView *view = [[UIImageView alloc] initWithImage:image];
  CHECK(sameStretchRect([view contentStretch], unit));
  [view setContentStretch:center];
  [view setFrame:CGRectMake(0, 0, 200, 100)];
  CHECK(sameStretchRect([view contentStretch], center));
  CHECK(sameStretchRect([[view layer] contentsCenter], center));
  CHECK([view frame].size.width == 200);
  CHECK((CGImageRef)[[view layer] contents] == [image CGImage]);
  CGRect strip = CGRectMake(0, 0.5, 1, 0);
  [view setContentStretch:strip];
  CHECK(sameStretchRect([view contentStretch], strip));
  [view setContentStretch:unit];
  CHECK(sameStretchRect([[view layer] contentsCenter], unit));
  [view release];
  [pool drain];
  return ok ? 0 : -1;
}

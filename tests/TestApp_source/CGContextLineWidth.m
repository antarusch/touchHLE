/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
#import "system_headers.h"
#include <stdio.h>
#include <string.h>

void CGContextSetLineWidth(CGContextRef c, CGFloat width);
void CGContextSetRGBStrokeColor(CGContextRef c, CGFloat r, CGFloat g, CGFloat b,
                                CGFloat a);
void CGContextSetGrayStrokeColor(CGContextRef c, CGFloat gray, CGFloat a);
void CGContextStrokeRect(CGContextRef c, CGRect rect);
void CGContextStrokeRectWithWidth(CGContextRef c, CGRect rect, CGFloat width);

static BOOL checkStroke(BOOL condition, int line) {
  if (!condition)
    printf("CGContext line width check failed at line %d\n", line);
  return condition;
}
#define CHECK(condition) ok &= checkStroke((condition), __LINE__)

static int paintedPixels(const unsigned char *pixels, int count) {
  int painted = 0;
  for (int i = 0; i < count; ++i)
    painted += pixels[i * 4 + 3] != 0;
  return painted;
}

int test_CGContext_line_width(void) {
  unsigned char pixels[8 * 8 * 4];
  memset(pixels, 0, sizeof(pixels));
  CGColorSpaceRef cs = CGColorSpaceCreateDeviceRGB();
  CGContextRef c = CGBitmapContextCreate(pixels, 8, 8, 8, 32, cs,
                                         kCGImageAlphaPremultipliedLast);
  BOOL ok = YES;
  CGContextStrokeRect(c, CGRectMake(2.5, 2.5, 3, 3));
  CHECK(paintedPixels(pixels, 64) == 12); // Default width is one unit.
  memset(pixels, 0, sizeof(pixels));
  CGContextSetLineWidth(c, 2);
  CGContextSaveGState(c);
  CGContextSetLineWidth(c, 4);
  CGContextSetRGBStrokeColor(c, 1, 0, 0, 1);
  CGContextSaveGState(c);
  CGContextSetLineWidth(c, 6);
  CGContextRestoreGState(c);
  CGContextStrokeRect(c, CGRectMake(2, 2, 4, 4));
  CHECK(paintedPixels(pixels, 64) == 64); // Inner restore recovers width 4.
  memset(pixels, 0, sizeof(pixels));
  CGContextRestoreGState(c);
  CGContextStrokeRect(c, CGRectMake(2, 2, 4, 4));
  CHECK(paintedPixels(pixels, 64) == 32);
  CHECK(pixels[(1 * 8 + 1) * 4] == 0); // Default stroke restored to black.
  CHECK(pixels[(1 * 8 + 1) * 4 + 3] == 255);
  CHECK(pixels[(3 * 8 + 3) * 4 + 3] == 0); // Interior remains empty.

  memset(pixels, 0, sizeof(pixels));
  CGContextSetLineWidth(c, 4);
  CGContextSetRGBStrokeColor(c, 1, 0, 0, 1);
  CGContextStrokeRectWithWidth(c, CGRectMake(2, 2, 4, 4), 2);
  CHECK(paintedPixels(pixels, 64) == 32);
  CHECK(pixels[(1 * 8 + 1) * 4] == 255);

  memset(pixels, 0, sizeof(pixels));
  CGContextStrokeRect(c, CGRectMake(2, 2, 4, 4));
  CHECK(paintedPixels(pixels, 64) == 64);

  memset(pixels, 0, sizeof(pixels));
  CGContextSetGrayStrokeColor(c, 1, 0.5);
  CGContextStrokeRect(c, CGRectMake(2, 2, 4, 4));
  CHECK(pixels[3] == 127); // A corner must be blended only once.
  CHECK(pixels[(3 * 8 + 3) * 4 + 3] == 127);

  memset(pixels, 0, sizeof(pixels));
  CGContextSetRGBFillColor(c, 0, 1, 0, 1);
  CGContextFillRect(c, CGRectMake(0, 0, 8, 8));
  CHECK(pixels[0] == 0 && pixels[1] == 255 && pixels[2] == 0);
  CGContextRelease(c);

  unsigned char scaled[16 * 8 * 4];
  memset(scaled, 0, sizeof(scaled));
  c = CGBitmapContextCreate(scaled, 16, 8, 8, 64, cs,
                            kCGImageAlphaPremultipliedLast);
  CGContextSetLineWidth(c, 2);
  CGContextScaleCTM(c, 2, 1);
  CGContextStrokeRect(c, CGRectMake(2, 2, 4, 4));
  CHECK(paintedPixels(scaled, 128) == 64);
  CGContextRelease(c);
  CGColorSpaceRelease(cs);
  return ok ? 0 : -1;
}

/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
#import "system_headers.h"
#include <stdio.h>

static NSUInteger animationViewDeallocations;
@interface ImageAnimationTestView : UIImageView
@end
@implementation ImageAnimationTestView
- (void)dealloc {
  animationViewDeallocations++;
  [super dealloc];
}
@end

@interface ImageHighlightTestCoder : NSCoder {
@public
  UIImage *normal;
  UIImage *highlight;
  BOOL interactive;
}
@end
@implementation ImageHighlightTestCoder
- (id)decodeObjectForKey:(NSString *)key {
  if ([key isEqualToString:[NSString stringWithUTF8String:"UIImage"]])
    return normal;
  if ([key
          isEqualToString:[NSString stringWithUTF8String:"UIHighlightedImage"]])
    return highlight;
  return nil;
}
- (BOOL)containsValueForKey:(NSString *)key {
  return [key
      isEqualToString:[NSString
                          stringWithUTF8String:"UIUserInteractionDisabled"]];
}
- (BOOL)decodeBoolForKey:(NSString *)key {
  if ([key isEqualToString:[NSString stringWithUTF8String:"UIHighlighted"]])
    return YES;
  if ([key isEqualToString:
               [NSString stringWithUTF8String:"UIUserInteractionDisabled"]])
    return !interactive;
  return NO;
}
- (NSInteger)decodeIntegerForKey:(NSString *)key {
  return 0;
}
- (CGRect)decodeCGRectForKey:(NSString *)key {
  return CGRectMake(0, 0, 20, 20);
}
- (CGPoint)decodeCGPointForKey:(NSString *)key {
  return CGPointMake(10, 10);
}
@end

static UIImage *makeHighlightImage(void) {
  CGColorSpaceRef cs = CGColorSpaceCreateDeviceRGB();
  CGContextRef context = CGBitmapContextCreate(NULL, 8, 8, 8, 32, cs,
                                               kCGImageAlphaPremultipliedLast);
  CGImageRef cgImage = CGBitmapContextCreateImage(context);
  UIImage *image = [UIImage imageWithCGImage:cgImage];
  CGImageRelease(cgImage);
  CGContextRelease(context);
  CGColorSpaceRelease(cs);
  return image;
}
static BOOL displaysImage(UIImageView *view, UIImage *image) {
  return (CGImageRef)[[view layer] contents] == [image CGImage];
}

static BOOL checkImageView(BOOL condition, int line) {
  if (!condition)
    printf("UIImageView check failed at line %d\n", line);
  return condition;
}
#define CHECK(condition) ok &= checkImageView((condition), __LINE__)

int test_UIImageView_highlight_and_animation(void) {
  NSAutoreleasePool *pool = [NSAutoreleasePool new];
  UIImage *normal = makeHighlightImage();
  UIImage *highlight = makeHighlightImage();
  UIImage *replacement = makeHighlightImage();
  UIImageView *view = [[UIImageView alloc] initWithImage:normal
                                        highlightedImage:highlight];
  CGRect initial = [view frame];
  BOOL ok = YES;
  CHECK([view image] == normal);
  CHECK([view highlightedImage] == highlight);
  CHECK(![view isHighlighted]);
  CHECK(![view isUserInteractionEnabled]);
  CHECK(initial.size.width == 8);
  CHECK(initial.size.height == 8);
  CHECK(displaysImage(view, normal));
  [view setHighlighted:YES];
  CHECK([view isHighlighted]);
  CHECK(displaysImage(view, highlight));
  [view setImage:replacement];
  CHECK([view image] == replacement);
  CHECK(displaysImage(view, highlight));
  [view setHighlightedImage:highlight]; // Same-object assignment is safe.
  [view setHighlightedImage:nil];
  CHECK(displaysImage(view, replacement));
  [view setHighlighted:NO];
  [view setImage:replacement];
  CGRect after = [view frame];
  CHECK(displaysImage(view, replacement));
  CHECK(after.size.width == 8);
  CHECK(after.size.height == 8);
  [view setImage:nil];
  CHECK([[view layer] contents] == nil);
  [view release];
  CHECK([normal retainCount] == 1);
  CHECK([highlight retainCount] == 1);
  CHECK([replacement retainCount] == 1);

  view = [[UIImageView alloc] initWithImage:nil highlightedImage:highlight];
  [view setHighlighted:YES];
  CHECK([view image] == nil);
  CHECK(displaysImage(view, highlight));
  [view release];
  view = [[UIImageView alloc] initWithImage:nil highlightedImage:nil];
  CHECK([view frame].size.width == 0);
  CHECK(![view isHighlighted]);
  [view release];

  ImageHighlightTestCoder *coder = [ImageHighlightTestCoder new];
  coder->normal = normal;
  coder->highlight = highlight;
  view = [[UIImageView alloc] initWithCoder:coder];
  CHECK([view isHighlighted]);
  CHECK(displaysImage(view, highlight));
  CHECK(![view isUserInteractionEnabled]);
  [view release];
  coder->interactive = YES;
  view = [[UIImageView alloc] initWithCoder:coder];
  CHECK([view isUserInteractionEnabled]);
  [view release];
  [coder release];

  UIImage *frame0 = makeHighlightImage();
  UIImage *frame1 = makeHighlightImage();
  NSArray *frames = [NSArray arrayWithObjects:frame0, frame1, nil];
  view = [[ImageAnimationTestView alloc] initWithImage:normal
                                      highlightedImage:highlight];
  CHECK([view animationImages] == nil);
  CHECK([view animationDuration] == 0.0);
  CHECK([view animationRepeatCount] == 0);
  CHECK(![view isAnimating]);
  [view startAnimating];
  CHECK(![view isAnimating]);
  [view setAnimationImages:frames];
  [view setAnimationDuration:1.0];
  [view setAnimationRepeatCount:2];
  CHECK([[view animationImages] count] == 2);
  CHECK([view animationRepeatCount] == 2);
  CHECK([view animationDuration] == 1.0);
  CHECK(displaysImage(view, normal));
  [view startAnimating];
  CHECK([view isAnimating]);
  CHECK(displaysImage(view, frame0));
  CHECK([view image] == normal);
  [view setHighlighted:YES];
  CHECK(displaysImage(view, highlight));
  [view setHighlighted:NO];
  CHECK(displaysImage(view, frame0));
  [view stopAnimating];
  CHECK(![view isAnimating]);
  CHECK(displaysImage(view, normal));
  [view setAnimationRepeatCount:0];
  [view startAnimating];
  [view setAnimationImages:nil];
  CHECK(![view isAnimating]);
  CHECK(displaysImage(view, normal));
  [view setAnimationImages:frames];
  [view startAnimating];
  // A repeating timer must not retain the image view forever.
  NSUInteger before = animationViewDeallocations;
  [view release];
  CHECK(animationViewDeallocations == before + 1);
  [pool drain];
  return ok ? 0 : -1;
}

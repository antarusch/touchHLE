/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
#import "system_headers.h"

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

int test_UIImageView_highlight_and_animation(void) {
  NSAutoreleasePool *pool = [NSAutoreleasePool new];
  UIImage *normal = makeHighlightImage();
  UIImage *highlight = makeHighlightImage();
  UIImage *replacement = makeHighlightImage();
  UIImageView *view = [[UIImageView alloc] initWithImage:normal
                                        highlightedImage:highlight];
  CGRect initial = [view frame];
  BOOL ok = [view image] == normal && [view highlightedImage] == highlight &&
            ![view isHighlighted] && ![view isUserInteractionEnabled] &&
            initial.size.width == 8 && initial.size.height == 8 &&
            displaysImage(view, normal);
  [view setHighlighted:YES];
  ok &= [view isHighlighted] && displaysImage(view, highlight);
  [view setImage:replacement];
  ok &= [view image] == replacement && displaysImage(view, highlight);
  [view setHighlightedImage:highlight]; // Same-object assignment is safe.
  [view setHighlightedImage:nil];
  ok &= displaysImage(view, replacement);
  [view setHighlighted:NO];
  [view setImage:replacement];
  CGRect after = [view frame];
  ok &= displaysImage(view, replacement) && after.size.width == 8 &&
        after.size.height == 8;
  [view setImage:nil];
  ok &= [[view layer] contents] == nil;
  [view release];
  ok &= [normal retainCount] == 1 && [highlight retainCount] == 1 &&
        [replacement retainCount] == 1;

  view = [[UIImageView alloc] initWithImage:nil highlightedImage:highlight];
  [view setHighlighted:YES];
  ok &= [view image] == nil && displaysImage(view, highlight);
  [view release];
  view = [[UIImageView alloc] initWithImage:nil highlightedImage:nil];
  ok &= [view frame].size.width == 0 && ![view isHighlighted];
  [view release];

  ImageHighlightTestCoder *coder = [ImageHighlightTestCoder new];
  coder->normal = normal;
  coder->highlight = highlight;
  view = [[UIImageView alloc] initWithCoder:coder];
  ok &= [view isHighlighted] && displaysImage(view, highlight) &&
        ![view isUserInteractionEnabled];
  [view release];
  coder->interactive = YES;
  view = [[UIImageView alloc] initWithCoder:coder];
  ok &= [view isUserInteractionEnabled];
  [view release];
  [coder release];

  UIImage *frame0 = makeHighlightImage();
  UIImage *frame1 = makeHighlightImage();
  NSArray *frames = [NSArray arrayWithObjects:frame0, frame1, nil];
  view = [[ImageAnimationTestView alloc] initWithImage:normal
                                      highlightedImage:highlight];
  ok &= [view animationImages] == nil && [view animationDuration] == 0.0 &&
        [view animationRepeatCount] == 0 && ![view isAnimating];
  [view startAnimating];
  ok &= ![view isAnimating];
  [view setAnimationImages:frames];
  [view setAnimationDuration:1.0];
  [view setAnimationRepeatCount:2];
  ok &= [[view animationImages] count] == 2 &&
        [view animationRepeatCount] == 2 && [view animationDuration] == 1.0 &&
        displaysImage(view, normal);
  [view startAnimating];
  ok &= [view isAnimating] && displaysImage(view, frame0) &&
        [view image] == normal;
  [view setHighlighted:YES];
  ok &= displaysImage(view, highlight);
  [view setHighlighted:NO];
  ok &= displaysImage(view, frame0);
  [view stopAnimating];
  ok &= ![view isAnimating] && displaysImage(view, normal);
  [view setAnimationRepeatCount:0];
  [view startAnimating];
  [view setAnimationImages:nil];
  ok &= ![view isAnimating] && displaysImage(view, normal);
  [view setAnimationImages:frames];
  [view startAnimating];
  // A repeating timer must not retain the image view forever.
  NSUInteger before = animationViewDeallocations;
  [view release];
  ok &= animationViewDeallocations == before + 1;
  [pool drain];
  return ok ? 0 : -1;
}

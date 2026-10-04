/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
#import "system_headers.h"

// Exercise the private archive shape used by image-only Interface Builder
// buttons, without including any game's assets or archives.
@interface ButtonContentTestCoder : NSCoder {
@public
  NSDictionary *values;
}
@end
@implementation ButtonContentTestCoder
- (id)decodeObjectForKey:(NSString *)key {
  return [values objectForKey:key];
}
- (int)decodeIntForKey:(NSString *)key {
  return 0;
}
- (NSInteger)decodeIntegerForKey:(NSString *)key {
  return 0;
}
- (BOOL)decodeBoolForKey:(NSString *)key {
  return NO;
}
- (CGRect)decodeCGRectForKey:(NSString *)key {
  return CGRectMake(0, 0, 230, 46);
}
- (CGPoint)decodeCGPointForKey:(NSString *)key {
  return CGPointMake(115, 23);
}
@end

static UIImage *makeButtonImage(NSUInteger width, NSUInteger height) {
  CGColorSpaceRef colorSpace = CGColorSpaceCreateDeviceRGB();
  CGContextRef context =
      CGBitmapContextCreate(NULL, width, height, 8, width * 4, colorSpace,
                            kCGImageAlphaPremultipliedLast);
  CGImageRef cgImage = CGBitmapContextCreateImage(context);
  UIImage *image = [UIImage imageWithCGImage:cgImage];
  CGImageRelease(cgImage);
  CGContextRelease(context);
  CGColorSpaceRelease(colorSpace);
  return image;
}
static id decodeButtonContent(UIImage *image, UIImage *background) {
  ButtonContentTestCoder *coder = [ButtonContentTestCoder new];
  coder->values = [NSDictionary
      dictionaryWithObjects:[NSArray arrayWithObjects:image, background, nil]
                    forKeys:[NSArray
                                arrayWithObjects:
                                    [NSString stringWithUTF8String:"UIImage"],
                                    [NSString stringWithUTF8String:
                                                  "UIBackgroundImage"],
                                    nil]];
  id content =
      [[NSClassFromString([NSString stringWithUTF8String:"UIButtonContent"])
          alloc] initWithCoder:coder];
  [coder release];
  return content;
}

int test_UIButton_archived_images(void) {
  NSAutoreleasePool *pool = [NSAutoreleasePool new];
  UIImage *normal = makeButtonImage(180, 40);
  UIImage *highlighted = makeButtonImage(200, 44);
  UIImage *background = makeButtonImage(1, 1);
  id normalContent = decodeButtonContent(normal, background);
  id highlightedContent = decodeButtonContent(highlighted, background);
  ButtonContentTestCoder *coder = [ButtonContentTestCoder new];
  NSDictionary *states = [NSDictionary
      dictionaryWithObjects:[NSArray arrayWithObjects:normalContent,
                                                      highlightedContent, nil]
                    forKeys:[NSArray
                                arrayWithObjects:[NSNumber numberWithInt:0],
                                                 [NSNumber numberWithInt:1],
                                                 nil]];
  coder->values = [NSDictionary
      dictionaryWithObjects:[NSArray arrayWithObjects:states, nil]
                    forKeys:[NSArray
                                arrayWithObjects:
                                    [NSString stringWithUTF8String:
                                                  "UIButtonStatefulContent"],
                                    nil]];
  UIButton *button = [[UIButton alloc] initWithCoder:coder];
  [normalContent release];
  [highlightedContent release];
  [coder release];
  [button layoutSubviews];
  CGRect frame = [[button imageView] frame];
  CGRect bgFrame = [[button backgroundImageView] frame];
  BOOL ok = [button currentTitle] == nil && [button currentImage] == normal &&
            [[button imageView] image] == normal &&
            [button backgroundImageForState:0] == background &&
            bgFrame.size.width == 230 && bgFrame.size.height == 46 &&
            frame.origin.x == 25 && frame.origin.y == 3 &&
            frame.size.width == 180 && frame.size.height == 40;
  [button setHighlighted:YES];
  frame = [[button imageView] frame];
  ok &= [button currentImage] == highlighted &&
        [[button imageView] image] == highlighted && frame.origin.x == 15 &&
        frame.origin.y == 1 && frame.size.width == 200 &&
        frame.size.height == 44;
  [button setHighlighted:NO];
  [button setEnabled:NO];
  ok &= [button currentImage] == normal;
  [button setEnabled:YES];
  [button setSelected:YES];
  ok &= [button currentImage] == normal;
  [button setSelected:NO];
  [button setImage:nil forState:0];
  ok &= [[button imageView] image] == nil;
  [button release];
  [pool drain];
  return ok ? 0 : -1;
}

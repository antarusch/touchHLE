/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

#import "system_headers.h"

@interface ViewLoadTestController : UIViewController {
@public
  NSUInteger loadCount;
  UIView *viewSeenInCallback;
  id outlet;
  id outletSeenInCallback;
}
@end

@implementation ViewLoadTestController
- (void)loadView {
  UIView *root = [[UIView alloc] initWithFrame:CGRectMake(0, 0, 32, 32)];
  [self setView:root];
  [root release];
}
- (void)viewDidLoad {
  loadCount++;
  // A callback must be able to access self.view without another callback.
  if (loadCount < 2)
    viewSeenInCallback = [self view];
  outletSeenInCallback = outlet;
}
@end

@interface ViewLoadTestCoder : NSCoder {
@public
  UIView *root;
}
@end

@implementation ViewLoadTestCoder
- (id)decodeObjectForKey:(NSString *)key {
  return [key isEqualToString:[NSString stringWithUTF8String:"UIView"]] ? root
                                                                        : nil;
}
@end

int test_UIViewController_viewDidLoad(void) {
  ViewLoadTestController *lazy = [[ViewLoadTestController alloc] init];
  UIView *first = [lazy view];
  BOOL lazyOK = first != nil && lazy->loadCount == 1 &&
                lazy->viewSeenInCallback == first && [lazy view] == first &&
                lazy->loadCount == 1;
  [lazy setView:nil];
  UIView *reloaded = [lazy view];
  BOOL reloadOK = reloaded != nil && lazy->loadCount == 2 &&
                  [lazy view] == reloaded && lazy->loadCount == 2;
  [lazy release];
  if (!lazyOK || !reloadOK)
    return -1;

  ViewLoadTestCoder *coder = [[ViewLoadTestCoder alloc] init];
  coder->root = [[UIView alloc] initWithFrame:CGRectMake(0, 0, 32, 32)];
  ViewLoadTestController *decoded =
      [[ViewLoadTestController alloc] initWithCoder:coder];
  BOOL deferred = decoded->loadCount == 0;
  // Simulate connecting nib outlets after decoding, before the first access.
  decoded->outlet = coder;
  BOOL decodedOK = [decoded view] == coder->root && decoded->loadCount == 1 &&
                   decoded->viewSeenInCallback == coder->root &&
                   decoded->outletSeenInCallback == coder &&
                   [decoded view] == coder->root && decoded->loadCount == 1;
  [decoded release];
  [coder->root release];
  [coder release];
  return deferred && decodedOK ? 0 : -2;
}

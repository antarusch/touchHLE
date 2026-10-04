/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

#import "system_headers.h"

static CAAnimation *scheduled[16];
static NSUInteger scheduledCount;

// Capture the animations committed through the real UIView/CATransaction
// path. Drive their delegate messages explicitly to keep the CLI test
// deterministic and independent of window rendering and wall-clock sleeps.
@interface TransitionTestLayer : CALayer
@end
@implementation TransitionTestLayer
- (void)addAnimation:(CAAnimation *)animation forKey:(NSString *)key {
  if (scheduledCount < 16)
    scheduled[scheduledCount++] = [animation retain];
}
@end

@interface TransitionTestView : UIView
@end
@implementation TransitionTestView
+ (Class)layerClass {
  return [TransitionTestLayer class];
}
@end

@interface TransitionTestDelegate : NSObject {
@public
  NSUInteger starts;
  NSUInteger stops;
  NSUInteger finishedWord;
  NSString *seenID;
  void *seenContext;
}
- (void)willStart:(NSString *)animationID context:(void *)context;
// Deliberately read the entire ARM argument word: BOOL YES must be 1,
// rather than an NSNumber pointer with an arbitrary low byte.
- (void)didStop:(NSString *)animationID
       finished:(NSUInteger)finished
        context:(void *)context;
@end
@implementation TransitionTestDelegate
- (void)willStart:(NSString *)animationID context:(void *)context {
  starts++;
  seenID = animationID;
  seenContext = context;
}
- (void)didStop:(NSString *)animationID
       finished:(NSUInteger)finished
        context:(void *)context {
  stops++;
  finishedWord = finished;
  seenID = animationID;
  seenContext = context;
}
@end

@interface NSObject (TransitionTestAnimationDelegate)
- (void)animationDidStart:(CAAnimation *)animation;
- (void)animationDidStop:(CAAnimation *)animation finished:(BOOL)finished;
@end

int test_UIView_transition(void) {
  NSAutoreleasePool *pool = [NSAutoreleasePool new];
  TransitionTestView *view =
      [[TransitionTestView alloc] initWithFrame:CGRectMake(0, 0, 32, 32)];
  [view setAlpha:0.75];
  TransitionTestDelegate *delegate = [TransitionTestDelegate new];
  NSString *animationID = [NSString stringWithUTF8String:"transition-test"];
  int contextValue = 42;
  NSTimeInterval before = [[NSProcessInfo processInfo] systemUptime];
  [UIView beginAnimations:animationID context:&contextValue];
  [UIView setAnimationDuration:1.25];
  [UIView setAnimationDelay:0.125];
  [UIView setAnimationDelegate:delegate];
  [UIView setAnimationWillStartSelector:@selector(willStart:context:)];
  [UIView setAnimationDidStopSelector:@selector(didStop:finished:context:)];
  [UIView setAnimationTransition:1 forView:view cache:YES];
  [UIView commitAnimations];
  NSTimeInterval after = [[NSProcessInfo processInfo] systemUptime];
  BOOL timing = scheduledCount == 1 && [scheduled[0] duration] == 1.25 &&
                [scheduled[0] beginTime] >= before + 0.125 &&
                [scheduled[0] beginTime] <= after + 0.125 &&
                [view alpha] == 0.75 && delegate->starts == 0 &&
                delegate->stops == 0;
  if (scheduledCount == 1) {
    id bridge = [scheduled[0] delegate];
    [bridge animationDidStart:scheduled[0]];
    [bridge animationDidStop:scheduled[0] finished:YES];
  }
  BOOL callback = delegate->starts == 1 && delegate->stops == 1 &&
                  delegate->finishedWord == 1 &&
                  delegate->seenID == animationID &&
                  delegate->seenContext == &contextValue;

  // All legacy transition values can schedule a transition without property
  // changes, with either caching hint. The None value creates no animation.
  for (NSInteger transition = 0; transition <= 4; transition++) {
    NSUInteger count = scheduledCount;
    [UIView beginAnimations:nil context:nil];
    [UIView setAnimationTransition:transition forView:view cache:NO];
    [UIView commitAnimations];
    timing &= scheduledCount == count + (transition != 0);
  }
  // A later None setting cancels a transition in the same block.
  NSUInteger count = scheduledCount;
  [UIView beginAnimations:nil context:nil];
  [UIView setAnimationTransition:1 forView:view cache:YES];
  [UIView setAnimationTransition:0 forView:view cache:NO];
  [UIView commitAnimations];
  timing &= scheduledCount == count;

  // Nested blocks keep separate transition configuration and durations.
  [UIView beginAnimations:nil context:nil];
  [UIView setAnimationDuration:0.5];
  [UIView setAnimationTransition:2 forView:view cache:YES];
  [UIView beginAnimations:nil context:nil];
  [UIView setAnimationDuration:0.25];
  [UIView setAnimationTransition:3 forView:view cache:NO];
  [UIView commitAnimations];
  [UIView commitAnimations];
  timing &= scheduledCount == count + 2;
  if (scheduledCount == count + 2)
    timing &= [scheduled[count] duration] == 0.25 &&
              [scheduled[count + 1] duration] == 0.5;

  for (NSUInteger i = 0; i < scheduledCount; i++)
    [scheduled[i] release];
  scheduledCount = 0;
  [delegate release];
  [view release];
  [pool drain];

  // Cached timing functions must remain valid after the first pool drains.
  pool = [NSAutoreleasePool new];
  view = [[TransitionTestView alloc] initWithFrame:CGRectMake(0, 0, 32, 32)];
  [UIView beginAnimations:nil context:nil];
  [UIView setAnimationTransition:1 forView:view cache:NO];
  [UIView commitAnimations];
  timing &= scheduledCount == 1;
  for (NSUInteger i = 0; i < scheduledCount; i++)
    [scheduled[i] release];
  scheduledCount = 0;
  [view release];
  [pool drain];
  return timing && callback ? 0 : -1;
}

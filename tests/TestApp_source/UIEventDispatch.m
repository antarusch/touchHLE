/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

#import "system_headers.h"

static NSUInteger applicationCalls;
static UIWindow *dispatchingWindow;
static BOOL callbackError;

@interface InputTestApplication : UIApplication
@end
@implementation InputTestApplication
- (void)sendEvent:(UIEvent *)event {
  applicationCalls++;
  [super sendEvent:event];
}
@end

@interface InputTestWindow : UIWindow {
@public
  NSUInteger calls;
  NSUInteger touchCount;
  BOOL suppress;
}
@end
@implementation InputTestWindow
- (void)sendEvent:(UIEvent *)event {
  if (applicationCalls == 0 || [event type] != 0)
    callbackError = YES;
  calls++;
  touchCount = [[event touchesForWindow:self] count];
  dispatchingWindow = self;
  if (!suppress)
    [super sendEvent:event];
  dispatchingWindow = nil;
}
@end

@interface InputTestView : UIView {
@public
  UIWindow *expectedWindow;
  NSUInteger calls[5];
  NSUInteger counts[5];
}
- (void)record:(NSSet *)touches phase:(NSUInteger)phase;
@end
@implementation InputTestView
- (void)record:(NSSet *)touches phase:(NSUInteger)phase {
  if (dispatchingWindow != expectedWindow)
    callbackError = YES;
  calls[phase]++;
  counts[phase] += [touches count];
}
- (void)touchesBegan:(NSSet *)touches withEvent:(UIEvent *)event {
  [self record:touches phase:0];
}
- (void)touchesMoved:(NSSet *)touches withEvent:(UIEvent *)event {
  [self record:touches phase:1];
}
- (void)touchesEnded:(NSSet *)touches withEvent:(UIEvent *)event {
  [self record:touches phase:3];
}
- (void)touchesCancelled:(NSSet *)touches withEvent:(UIEvent *)event {
  [self record:touches phase:4];
}
@end

@interface InputTestTouch : UITouch {
@public
  UIWindow *testWindow;
  UIView *testView;
  NSInteger testPhase;
}
@end
@implementation InputTestTouch
- (UIWindow *)window {
  return testWindow;
}
- (UIView *)view {
  return testView;
}
- (NSInteger)phase {
  return testPhase;
}
@end

@interface InputTestEvent : UIEvent {
@public
  NSSet *testTouches;
}
@end
@implementation InputTestEvent
- (NSSet *)allTouches {
  return testTouches;
}
@end

static InputTestTouch *addTouch(NSMutableSet *set, UIWindow *window,
                                UIView *view, NSInteger phase) {
  InputTestTouch *touch = [[InputTestTouch new] autorelease];
  touch->testWindow = window;
  touch->testView = view;
  touch->testPhase = phase;
  [set addObject:touch];
  return touch;
}

int test_UIEvent_dispatch(void) {
  NSAutoreleasePool *pool = [NSAutoreleasePool new];
  InputTestApplication *app = [InputTestApplication new];
  InputTestWindow *a =
      [[InputTestWindow alloc] initWithFrame:CGRectMake(0, 0, 32, 32)];
  InputTestWindow *b =
      [[InputTestWindow alloc] initWithFrame:CGRectMake(0, 0, 32, 32)];
  InputTestView *va =
      [[InputTestView alloc] initWithFrame:CGRectMake(0, 0, 32, 32)];
  InputTestView *vb =
      [[InputTestView alloc] initWithFrame:CGRectMake(0, 0, 32, 32)];
  UIView *child = [[UIView alloc] initWithFrame:CGRectMake(0, 0, 16, 16)];
  [va addSubview:child];
  [va setMultipleTouchEnabled:YES];
  va->expectedWindow = a;
  vb->expectedWindow = b;
  NSMutableSet *touches = [[NSMutableSet new] autorelease];
  addTouch(touches, a, va, 0);
  addTouch(touches, a, va, 0);
  addTouch(touches, a, va, 1);
  InputTestTouch *ended = addTouch(touches, a, va, 3);
  addTouch(touches, a, va, 2);
  addTouch(touches, a, child, 4);
  addTouch(touches, a, nil, 0);
  InputTestTouch *other = addTouch(touches, b, vb, 0);
  addTouch(touches, nil, nil, 0);
  InputTestEvent *event = [InputTestEvent new];
  event->testTouches = touches;

  BOOL filtering = [[event touchesForWindow:a] count] == 7 &&
                   [[event touchesForWindow:b] count] == 1 &&
                   [[event touchesForView:va] count] == 5;
  [app sendEvent:event];
  BOOL phases = applicationCalls == 1 && a->calls == 1 && b->calls == 1 &&
                a->touchCount == 7 && b->touchCount == 1 && va->calls[0] == 1 &&
                va->counts[0] == 2 && va->calls[1] == 1 && va->counts[1] == 1 &&
                va->calls[3] == 1 && va->counts[3] == 1 && va->calls[4] == 1 &&
                va->counts[4] == 1 && va->calls[2] == 0 && vb->calls[0] == 1;

  // Stationary touches remain visible in the event, but are not dispatched.
  NSMutableSet *second = [[NSMutableSet new] autorelease];
  other->testPhase = 2;
  [second addObject:other];
  [second addObject:ended];
  event->testTouches = second;
  [app sendEvent:event];
  BOOL stationary = a->calls == 2 && b->calls == 1 && va->calls[3] == 2;
  // A window override must be able to intercept without forwarding to views.
  a->suppress = YES;
  [app sendEvent:event];
  BOOL interception = a->calls == 3 && b->calls == 1 && va->calls[3] == 2;

  [event release];
  [pool drain];
  [child release];
  [va release];
  [vb release];
  [a release];
  [b release];
  return filtering && phases && stationary && interception && !callbackError
             ? 0
             : -1;
}

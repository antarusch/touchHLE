/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIScrollView`.

pub mod ui_text_view;
use crate::frameworks::core_graphics::{CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::ns_string::get_static_str;
use crate::frameworks::foundation::NSInteger;
use crate::frameworks::uikit::ui_geometry::UIEdgeInsets;
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, nil, objc_classes, ClassExports, NSZonePtr, SEL,
};

type UIScrollViewIndicatorStyle = NSInteger;

pub struct UIScrollViewHostObject {
    superclass: super::UIViewHostObject,
    /// UIScrollViewDelegate, weak reference
    delegate: id,
    scroll_enabled: bool,
    delays_content_touches: bool,
    bounces: bool,
    always_bounce_vertical: bool,
    always_bounce_horizontal: bool,
    directional_lock_enabled: bool,
    paging_enabled: bool,
    shows_horizontal_scroll_indicator: bool,
    shows_vertical_scroll_indicator: bool,
    scrolls_to_top: bool,
    indicator_style: UIScrollViewIndicatorStyle,
    content_offset: CGPoint,
    content_size: CGSize,
    content_inset: UIEdgeInsets,
}
impl_HostObject_with_superclass!(UIScrollViewHostObject);
impl Default for UIScrollViewHostObject {
    fn default() -> Self {
        UIScrollViewHostObject {
            superclass: Default::default(),
            delegate: nil,
            scroll_enabled: true,
            delays_content_touches: true,
            bounces: true,
            always_bounce_vertical: false,
            always_bounce_horizontal: false,
            directional_lock_enabled: false,
            paging_enabled: false,
            shows_horizontal_scroll_indicator: true,
            shows_vertical_scroll_indicator: true,
            scrolls_to_top: true,
            indicator_style: 0,
            content_offset: CGPoint { x: 0.0, y: 0.0 },
            content_size: CGSize {
                width: 0.0,
                height: 0.0,
            },
            content_inset: Default::default(),
        }
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIScrollView: UIView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UIScrollViewHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithCoder:(id)coder {
    let this: id = crate::msg_super![env; this initWithCoder:coder];
    let key = get_static_str(env, "UIContentSize");
    let size: CGSize = msg![env; coder decodeCGSizeForKey:key];
    env.objc
        .borrow_mut::<UIScrollViewHostObject>(this)
        .content_size = size;
    () = msg![env; this setClipsToBounds:true];
    this
}
- (id)initWithFrame:(CGRect)frame {
    let this: id = crate::msg_super![env; this initWithFrame:frame];
    () = msg![env; this setClipsToBounds:true];
    this
}

- (id)delegate {
    env.objc.borrow::<UIScrollViewHostObject>(this).delegate
}
- (())setDelegate:(id)delegate {
    env.objc.borrow_mut::<UIScrollViewHostObject>(this).delegate = delegate;
}

- (bool)delaysContentTouches {
    env.objc
        .borrow::<UIScrollViewHostObject>(this)
        .delays_content_touches
}
- (())setDelaysContentTouches:(bool)value {
    env.objc
        .borrow_mut::<UIScrollViewHostObject>(this)
        .delays_content_touches = value;
}

- (bool)bounces {
    env.objc.borrow::<UIScrollViewHostObject>(this).bounces
}
- (())setBounces:(bool)value {
    env.objc.borrow_mut::<UIScrollViewHostObject>(this).bounces = value;
}

- (bool)alwaysBounceVertical {
    env.objc
        .borrow::<UIScrollViewHostObject>(this)
        .always_bounce_vertical
}
- (())setAlwaysBounceVertical:(bool)value {
    env.objc
        .borrow_mut::<UIScrollViewHostObject>(this)
        .always_bounce_vertical = value;
}

- (bool)alwaysBounceHorizontal {
    env.objc
        .borrow::<UIScrollViewHostObject>(this)
        .always_bounce_horizontal
}
- (())setAlwaysBounceHorizontal:(bool)value {
    env.objc
        .borrow_mut::<UIScrollViewHostObject>(this)
        .always_bounce_horizontal = value;
}

- (bool)scrollEnabled {
    env.objc.borrow::<UIScrollViewHostObject>(this).scroll_enabled
}
- (())setScrollEnabled:(bool)scroll_enabled {
    env.objc.borrow_mut::<UIScrollViewHostObject>(this).scroll_enabled = scroll_enabled;
}

- (bool)isDirectionalLockEnabled {
    env.objc
        .borrow::<UIScrollViewHostObject>(this)
        .directional_lock_enabled
}
- (())setDirectionalLockEnabled:(bool)enabled {
    env.objc
        .borrow_mut::<UIScrollViewHostObject>(this)
        .directional_lock_enabled = enabled;
}

- (bool)isPagingEnabled {
    env.objc.borrow::<UIScrollViewHostObject>(this).paging_enabled
}
- (())setPagingEnabled:(bool)enabled {
    env.objc.borrow_mut::<UIScrollViewHostObject>(this).paging_enabled = enabled;
}

- (bool)showsHorizontalScrollIndicator {
    env.objc
        .borrow::<UIScrollViewHostObject>(this)
        .shows_horizontal_scroll_indicator
}
- (())setShowsHorizontalScrollIndicator:(bool)value {
    env.objc
        .borrow_mut::<UIScrollViewHostObject>(this)
        .shows_horizontal_scroll_indicator = value;
}

- (bool)showsVerticalScrollIndicator {
    env.objc
        .borrow::<UIScrollViewHostObject>(this)
        .shows_vertical_scroll_indicator
}
- (())setShowsVerticalScrollIndicator:(bool)value {
    env.objc
        .borrow_mut::<UIScrollViewHostObject>(this)
        .shows_vertical_scroll_indicator = value;
}

- (bool)scrollsToTop {
    env.objc.borrow::<UIScrollViewHostObject>(this).scrolls_to_top
}
- (())setScrollsToTop:(bool)value {
    env.objc.borrow_mut::<UIScrollViewHostObject>(this).scrolls_to_top = value;
}

- (CGPoint)contentOffset {
    env.objc.borrow::<UIScrollViewHostObject>(this).content_offset
}
- (())setContentOffset:(CGPoint)offset {
    env.objc.borrow_mut::<UIScrollViewHostObject>(this).content_offset = offset;
    // Bounds origin should be equals to the content offset
    let mut bounds: CGRect = msg![env; this bounds];
    bounds.origin = offset;
    () = msg![env; this setBounds:bounds];
    () = msg![env; this setNeedsDisplay];
}

- (())setContentOffset:(CGPoint)offset animated:(bool)animated {
    if animated {
        () = crate::msg_class![env; UIView beginAnimations:nil context:(crate::mem::ConstVoidPtr::null())];
        () = crate::msg_class![env; UIView setAnimationDuration:0.25f64];
    }
    () = msg![env; this setContentOffset:offset];
    if animated {
        () = crate::msg_class![env; UIView commitAnimations];
    }
}
- (UIEdgeInsets)contentInset {
    env.objc
        .borrow::<UIScrollViewHostObject>(this)
        .content_inset
}
- (())setContentInset:(UIEdgeInsets)value {
    env.objc
        .borrow_mut::<UIScrollViewHostObject>(this)
        .content_inset = value;
    () = msg![env; this setNeedsDisplay];
}

- (CGSize)contentSize {
    env.objc.borrow::<UIScrollViewHostObject>(this).content_size
}
- (())setContentSize:(CGSize)size {
    env.objc.borrow_mut::<UIScrollViewHostObject>(this).content_size = size;
}

- (UIScrollViewIndicatorStyle)indicatorStyle {
    env.objc.borrow::<UIScrollViewHostObject>(this).indicator_style
}
- (())setIndicatorStyle:(UIScrollViewIndicatorStyle)style {
    env.objc.borrow_mut::<UIScrollViewHostObject>(this).indicator_style = style;
}

- (())touchesMoved:(id)touches // NSSet* of UITouch*
         withEvent:(id)_event {
    // UIEvent*
    let scroll_enabled: bool = msg![env; this scrollEnabled];
    if !scroll_enabled {
        return;
    }

    let touch_arr: id = msg![env; touches allObjects];
    // Assume single finger touches for now
    let touch: id = msg![env; touch_arr objectAtIndex:0u32];
    let bounds: CGRect = msg![env; this bounds];

    let prev_location: CGPoint = msg![env; touch previousLocationInView:this];
    let prev_x = prev_location.x;
    let prev_y = prev_location.y;

    let new_location: CGPoint = msg![env; touch locationInView:this];
    let y = new_location.y;
    let x = new_location.x;

    let delta_y = y - prev_y;
    let delta_x = x - prev_x;

    let offset: CGPoint = msg![env; this contentOffset];
    let content_size: CGSize = msg![env; this contentSize];

    // Very rudimentary scrolling.
    // We emulate sliding up to scroll down like on the real iPhone.
    let mut new_content_offset: CGPoint = CGPoint {
        x: offset.x - delta_x,
        y: offset.y - delta_y,
    };

    // Update content offset within bounds
    let inset: UIEdgeInsets = msg![env; this contentInset];
    new_content_offset.y = new_content_offset
        .y
        .min((content_size.height - bounds.size.height + inset.bottom).max(-inset.top))
        .max(-inset.top);
    new_content_offset.x = new_content_offset
        .x
        .min((content_size.width - bounds.size.width + inset.right).max(-inset.left))
        .max(-inset.left);

    // Trigger rerender only if required.
    log_dbg!(
        "content offset: old {:?}, new {:?}",
        offset,
        new_content_offset
    );
    if new_content_offset != offset {
        () = msg![env; this setContentOffset:new_content_offset];

        let delegate: id = msg![env; this delegate];
        let sel: SEL = env
            .objc
            .register_host_selector("scrollViewDidScroll:".to_string(), &mut env.mem);
        let responds: bool = msg![env; delegate respondsToSelector:sel];
        if responds {
            () = msg![env; delegate scrollViewDidScroll:this];
        }
    }
}

@end

};

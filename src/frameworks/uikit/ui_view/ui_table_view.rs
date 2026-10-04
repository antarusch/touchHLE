/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Table rows, reusable cells, NIB decoding and datasource/delegate callbacks.

use super::ui_scroll_view::UIScrollViewHostObject;
use super::UIViewHostObject;
use crate::frameworks::core_graphics::{CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::ns_string::get_static_str;
use crate::frameworks::foundation::NSInteger;
use crate::objc::{
    autorelease, id, msg, msg_class, msg_super, nil, objc_classes, release, retain, ClassExports,
    NSZonePtr,
};
use crate::{impl_HostObject_with_superclass, Environment};

#[derive(Default)]
struct TableHostObject {
    superclass: UIScrollViewHostObject,
    data_source: id,
    row_height: f32,
    separator_color: id,
    separator_style: NSInteger,
    background_view: id,
    rows: Vec<(id, id, CGRect)>, // retained cells and index paths
    reuse_pool: Vec<id>,
    selected_row: id,
    touch_start: Option<CGPoint>,
    dragged: bool,
    reloading: bool,
}
impl_HostObject_with_superclass!(TableHostObject);

#[derive(Default)]
struct CellHostObject {
    superclass: UIViewHostObject,
    reuse_identifier: id,
    content_view: id,
    background_view: id,
    selected_background_view: id,
    text_label: id,
    detail_text_label: id,
    selected: bool,
}
impl_HostObject_with_superclass!(CellHostObject);

fn responds(env: &mut Environment, object: id, name: &str) -> bool {
    if object == nil {
        return false;
    }
    let selector = env.objc.register_host_selector(name.into(), &mut env.mem);
    msg![env; object respondsToSelector:selector]
}

fn init_cell_content(env: &mut Environment, cell: id) {
    if env.objc.borrow::<CellHostObject>(cell).content_view == nil {
        let bounds: CGRect = msg![env; cell bounds];
        let view: id = msg_class![env; UITableViewCellContentView alloc];
        let view: id = msg![env; view initWithFrame:bounds];
        env.objc.borrow_mut::<CellHostObject>(cell).content_view = view;
        () = msg![env; cell addSubview:view];
    }
}

fn replace_view(env: &mut Environment, cell: id, view: id, selected: bool) {
    retain(env, view);
    let host = env.objc.borrow_mut::<CellHostObject>(cell);
    let old = std::mem::replace(
        if selected {
            &mut host.selected_background_view
        } else {
            &mut host.background_view
        },
        view,
    );
    () = msg![env; old removeFromSuperview];
    release(env, old);
    if view != nil {
        let bounds: CGRect = msg![env; cell bounds];
        () = msg![env; view setFrame:bounds];
        if selected {
            let content: id = msg![env; cell contentView];
            () = msg![env; cell insertSubview:view belowSubview:content];
        } else {
            () = msg![env; cell insertSubview:view atIndex:0u32];
        }
        if selected {
            let is_selected = env.objc.borrow::<CellHostObject>(cell).selected;
            () = msg![env; view setHidden:(!is_selected)];
        }
    }
}

pub const CLASSES: ClassExports = objc_classes! {
(env, this, _cmd);

@implementation UITableView: UIScrollView
+ (id)allocWithZone:(NSZonePtr)_zone {
    let host = Box::new(TableHostObject {
        row_height: 44.0,
        separator_style: 1,
        ..Default::default()
    });
    env.objc.alloc_object(this, host, &mut env.mem)
}
- (id)initWithFrame:(CGRect)frame style:(NSInteger)_style {
    msg![env; this initWithFrame:frame]
}
- (id)initWithCoder:(id)coder {
    let this: id = msg_super![env; this initWithCoder:coder];
    let key = get_static_str(env, "UIRowHeight");
    let height: f32 = msg![env; coder decodeFloatForKey:key];
    if height > 0.0 {
        env.objc.borrow_mut::<TableHostObject>(this).row_height = height;
    }
    let key = get_static_str(env, "UISeparatorStyle");
    let style: NSInteger = msg![env; coder decodeIntegerForKey:key];
    env.objc.borrow_mut::<TableHostObject>(this).separator_style = style;
    this
}
- (id)dataSource {
    env.objc.borrow::<TableHostObject>(this).data_source
}
- (())setDataSource:(id)source {
    env.objc.borrow_mut::<TableHostObject>(this).data_source = source;
}
- (f32)rowHeight {
    env.objc.borrow::<TableHostObject>(this).row_height
}
- (())setRowHeight:(f32)height {
    env.objc.borrow_mut::<TableHostObject>(this).row_height = height.max(1.0);
}
- (NSInteger)separatorStyle {
    env.objc.borrow::<TableHostObject>(this).separator_style
}
- (())setSeparatorStyle:(NSInteger)style {
    env.objc.borrow_mut::<TableHostObject>(this).separator_style = style;
    () = msg![env; this setNeedsDisplay];
}
- (())setSeparatorColor:(id)color {
    retain(env, color);
    let old = std::mem::replace(
        &mut env.objc.borrow_mut::<TableHostObject>(this).separator_color,
        color,
    );
    release(env, old);
    () = msg![env; this setNeedsDisplay];
}
- (id)separatorColor {
    env.objc.borrow::<TableHostObject>(this).separator_color
}
- (id)backgroundView {
    env.objc.borrow::<TableHostObject>(this).background_view
}
- (())setBackgroundView:(id)view {
    retain(env, view);
    let old = std::mem::replace(
        &mut env.objc.borrow_mut::<TableHostObject>(this).background_view,
        view,
    );
    () = msg![env; old removeFromSuperview];
    release(env, old);
    if view != nil {
        let bounds: CGRect = msg![env; this bounds];
        () = msg![env; view setFrame:bounds];
        () = msg![env; this insertSubview:view atIndex:0u32];
    }
}
- (id)dequeueReusableCellWithIdentifier:(id)identifier {
    let pool = env.objc.borrow::<TableHostObject>(this).reuse_pool.clone();
    for (index, cell) in pool.into_iter().enumerate() {
        let reuse: id = msg![env; cell reuseIdentifier];
        if msg![env; identifier isEqual:reuse] {
            let cell = env
                .objc
                .borrow_mut::<TableHostObject>(this)
                .reuse_pool
                .remove(index);
            () = msg![env; cell prepareForReuse];
            return autorelease(env, cell);
        }
    }
    nil
}
- (())reloadData {
    if env.objc.borrow::<TableHostObject>(this).reloading {
        return;
    }
    env.objc.borrow_mut::<TableHostObject>(this).reloading = true;
    let old = std::mem::take(&mut env.objc.borrow_mut::<TableHostObject>(this).rows);
    for (cell, path, _) in old {
        () = msg![env; cell removeFromSuperview];
        env.objc
            .borrow_mut::<TableHostObject>(this)
            .reuse_pool
            .push(cell);
        release(env, path);
    }
    let source = env.objc.borrow::<TableHostObject>(this).data_source;
    let delegate: id = msg![env; this delegate];
    let bounds: CGRect = msg![env; this bounds];
    let height = env.objc.borrow::<TableHostObject>(this).row_height;
    let sections: NSInteger = if responds(env, source, "numberOfSectionsInTableView:") {
        msg![env; source numberOfSectionsInTableView:this]
    } else {
        1
    };
    let mut y = 0.0f32;
    let mut rows = Vec::new();
    for section in 0..sections.max(0) {
        let count: NSInteger = msg![env; source tableView:this numberOfRowsInSection:section];
        for row in 0..count.max(0) {
            let path: id = msg_class![env; NSIndexPath indexPathForRow:row inSection:section];
            let cell: id = msg![env; source tableView:this cellForRowAtIndexPath:path];
            assert_ne!(cell, nil, "Table datasource returned a nil cell");
            let row_height: f32 = if responds(env, delegate, "tableView:heightForRowAtIndexPath:") {
                msg![env; delegate tableView:this heightForRowAtIndexPath:path]
            } else {
                height
            };
            let rect = CGRect {
                origin: CGPoint { x: 0.0, y },
                size: CGSize {
                    width: bounds.size.width,
                    height: row_height.max(1.0),
                },
            };
            retain(env, cell);
            retain(env, path);
            let mut cell_rect = rect;
            if env.objc.borrow::<TableHostObject>(this).separator_style != 0 {
                cell_rect.size.height = (cell_rect.size.height - 1.0).max(0.0);
            }
            () = msg![env; cell setFrame:cell_rect];
            () = msg![env; cell layoutSubviews];
            () = msg![env; this addSubview:cell];
            if responds(
                env,
                delegate,
                "tableView:willDisplayCell:forRowAtIndexPath:",
            ) {
                () = msg![env; delegate tableView:this willDisplayCell:cell forRowAtIndexPath:path];
            }
            rows.push((cell, path, rect));
            y += rect.size.height;
        }
    }
    let old_selected = env.objc.borrow_mut::<TableHostObject>(this).selected_row;
    env.objc.borrow_mut::<TableHostObject>(this).selected_row = nil;
    release(env, old_selected);
    env.objc.borrow_mut::<TableHostObject>(this).rows = rows;
    env.objc.borrow_mut::<TableHostObject>(this).reloading = false;
    () = msg![env; this setContentSize:(CGSize { width: bounds.size.width, height: y })];
    () = msg![env; this setNeedsDisplay];
}
- (id)indexPathForSelectedRow {
    env.objc.borrow::<TableHostObject>(this).selected_row
}
- (id)cellForRowAtIndexPath:(id)path {
    let rows = env.objc.borrow::<TableHostObject>(this).rows.clone();
    for (cell, candidate, _) in rows {
        if msg![env; path isEqual:candidate] {
            return cell;
        }
    }
    nil
}
- (id)indexPathForRowAtPoint:(CGPoint)point {
    let rows = &env.objc.borrow::<TableHostObject>(this).rows;
    rows.iter()
        .find(|(_, _, rect)| {
            point.x >= rect.origin.x
                && point.y >= rect.origin.y
                && point.x < rect.origin.x + rect.size.width
                && point.y < rect.origin.y + rect.size.height
        })
        .map(|(_, path, _)| *path)
        .unwrap_or(nil)
}
- (())selectRowAtIndexPath:(id)path animated:(bool)animated scrollPosition:(NSInteger)position {
    retain(env, path);
    let old = std::mem::replace(
        &mut env.objc.borrow_mut::<TableHostObject>(this).selected_row,
        path,
    );
    release(env, old);
    let rows = env.objc.borrow::<TableHostObject>(this).rows.clone();
    for (cell, candidate, rect) in rows {
        let selected: bool = msg![env; path isEqual:candidate];
        () = msg![env; cell setSelected:selected animated:animated];
        if selected && position != 0 {
            let bounds: CGRect = msg![env; this bounds];
            let y = match position {
                2 => rect.origin.y - (bounds.size.height - rect.size.height) * 0.5,
                3 => rect.origin.y - bounds.size.height + rect.size.height,
                _ => rect.origin.y,
            };
            () = msg![env; this setContentOffset:(CGPoint { x: 0.0, y: y.max(0.0) }) animated:animated];
        }
    }
}
- (())deselectRowAtIndexPath:(id)path animated:(bool)animated {
    let selected = env.objc.borrow::<TableHostObject>(this).selected_row;
    if msg![env; path isEqual:selected] {
        env.objc.borrow_mut::<TableHostObject>(this).selected_row = nil;
        release(env, selected);
    }
    let cell: id = msg![env; this cellForRowAtIndexPath:path];
    () = msg![env; cell setSelected:false animated:animated];
}
- (())touchesBegan:(id)touches withEvent:(id)_event {
    let touch: id = msg![env; touches anyObject];
    let point: CGPoint = msg![env; touch locationInView:nil];
    let host = env.objc.borrow_mut::<TableHostObject>(this);
    host.touch_start = Some(point);
    host.dragged = false;
}
- (())touchesMoved:(id)touches withEvent:(id)event {
    let touch: id = msg![env; touches anyObject];
    let point: CGPoint = msg![env; touch locationInView:nil];
    let host = env.objc.borrow_mut::<TableHostObject>(this);
    if host
        .touch_start
        .is_some_and(|start| (point.x - start.x).abs() > 5.0 || (point.y - start.y).abs() > 5.0)
    {
        host.dragged = true;
    }
    () = msg_super![env; this touchesMoved:touches withEvent:event];
}
- (())touchesEnded:(id)touches withEvent:(id)_event {
    if env.objc.borrow::<TableHostObject>(this).dragged {
        return;
    }
    let touch: id = msg![env; touches anyObject];
    let point: CGPoint = msg![env; touch locationInView:this];
    let mut path: id = msg![env; this indexPathForRowAtPoint:point];
    if path == nil {
        return;
    }
    let delegate: id = msg![env; this delegate];
    if responds(env, delegate, "tableView:willSelectRowAtIndexPath:") {
        path = msg![env; delegate tableView:this willSelectRowAtIndexPath:path];
    }
    if path == nil {
        return;
    }
    () = msg![env; this selectRowAtIndexPath:path animated:false scrollPosition:0i32];
    if responds(env, delegate, "tableView:didSelectRowAtIndexPath:") {
        () = msg![env; delegate tableView:this didSelectRowAtIndexPath:path];
    }
}
- (())drawRect:(CGRect)_rect {
    let host = env.objc.borrow::<TableHostObject>(this);
    if host.separator_style == 0 {
        return;
    }
    let color = host.separator_color;
    let rows = host.rows.clone();
    let color: id = if color == nil {
        msg_class![env; UIColor lightGrayColor]
    } else {
        color
    };
    () = msg![env; color setFill];
    let context = crate::frameworks::uikit::ui_graphics::UIGraphicsGetCurrentContext(env);
    let offset: CGPoint = msg![env; this contentOffset];
    for (_, _, rect) in rows {
        crate::frameworks::core_graphics::cg_context::CGContextFillRect(
            env,
            context,
            CGRect {
                origin: CGPoint {
                    x: 0.0,
                    y: rect.origin.y + rect.size.height - 1.0 - offset.y,
                },
                size: CGSize {
                    width: rect.size.width,
                    height: 1.0,
                },
            },
        );
    }
}
- (())dealloc {
    let host = env.objc.borrow_mut::<TableHostObject>(this);
    let rows = std::mem::take(&mut host.rows);
    let pool = std::mem::take(&mut host.reuse_pool);
    let refs = [
        host.separator_color,
        host.background_view,
        host.selected_row,
    ];
    for (cell, path, _) in rows {
        release(env, cell);
        release(env, path);
    }
    for cell in pool {
        release(env, cell);
    }
    for value in refs {
        release(env, value);
    }
    msg_super![env; this dealloc]
}
@end

@implementation UITableViewCell: UIView
+ (id)allocWithZone:(NSZonePtr)_zone {
    env.objc
        .alloc_object(this, Box::<CellHostObject>::default(), &mut env.mem)
}
- (id)initWithStyle:(NSInteger)_style reuseIdentifier:(id)identifier {
    let this: id = msg_super![env; this initWithFrame:(CGRect::default())];
    let identifier: id = msg![env; identifier copy];
    env.objc.borrow_mut::<CellHostObject>(this).reuse_identifier = identifier;
    init_cell_content(env, this);
    this
}
- (id)initWithFrame:(CGRect)frame reuseIdentifier:(id)identifier {
    let this: id = msg_super![env; this initWithFrame:frame];
    let identifier: id = msg![env; identifier copy];
    env.objc.borrow_mut::<CellHostObject>(this).reuse_identifier = identifier;
    init_cell_content(env, this);
    this
}
- (id)initWithCoder:(id)coder {
    let this: id = msg_super![env; this initWithCoder:coder];
    let key = get_static_str(env, "UIReuseIdentifier");
    let identifier: id = msg![env; coder decodeObjectForKey:key];
    let key = get_static_str(env, "UIContentView");
    let content: id = msg![env; coder decodeObjectForKey:key];
    let key = get_static_str(env, "UITextLabel");
    let label: id = msg![env; coder decodeObjectForKey:key];
    let key = get_static_str(env, "UIDetailTextLabel");
    let detail: id = msg![env; coder decodeObjectForKey:key];
    for value in [identifier, content, label, detail] {
        retain(env, value);
    }
    let host = env.objc.borrow_mut::<CellHostObject>(this);
    host.reuse_identifier = identifier;
    host.content_view = content;
    host.text_label = label;
    host.detail_text_label = detail;
    init_cell_content(env, this);
    this
}
- (id)contentView {
    init_cell_content(env, this);
    env.objc.borrow::<CellHostObject>(this).content_view
}
- (id)reuseIdentifier {
    env.objc.borrow::<CellHostObject>(this).reuse_identifier
}
- (id)backgroundView {
    env.objc.borrow::<CellHostObject>(this).background_view
}
- (())setBackgroundView:(id)view {
    replace_view(env, this, view, false);
}
- (id)selectedBackgroundView {
    env.objc
        .borrow::<CellHostObject>(this)
        .selected_background_view
}
- (())setSelectedBackgroundView:(id)view {
    replace_view(env, this, view, true);
}
- (bool)isSelected {
    env.objc.borrow::<CellHostObject>(this).selected
}
- (())setSelected:(bool)selected {
    () = msg![env; this setSelected:selected animated:false];
}
- (())setSelected:(bool)selected animated:(bool)_animated {
    if selected
        && env
            .objc
            .borrow::<CellHostObject>(this)
            .selected_background_view
            == nil
    {
        let bounds: CGRect = msg![env; this bounds];
        let background: id = msg_class![env; UIView alloc];
        let background: id = msg![env; background initWithFrame:bounds];
        let color: id =
            msg_class![env; UIColor colorWithRed:0.15f32 green:0.35f32 blue:0.75f32 alpha:1.0f32];
        () = msg![env; background setBackgroundColor:color];
        replace_view(env, this, background, true);
        release(env, background);
    }
    env.objc.borrow_mut::<CellHostObject>(this).selected = selected;
    let background = env
        .objc
        .borrow::<CellHostObject>(this)
        .selected_background_view;
    () = msg![env; background setHidden:(!selected)];
}
- (())prepareForReuse {
    () = msg![env; this setSelected:false];
}
- (())layoutSubviews {
    let bounds: CGRect = msg![env; this bounds];
    let host = env.objc.borrow::<CellHostObject>(this);
    let views = [
        host.content_view,
        host.background_view,
        host.selected_background_view,
    ];
    for view in views {
        () = msg![env; view setFrame:bounds];
    }
}
- (id)textLabel {
    let label = env.objc.borrow::<CellHostObject>(this).text_label;
    if label != nil {
        return label;
    }
    let bounds: CGRect = msg![env; this bounds];
    let label: id = msg_class![env; UILabel alloc];
    let label: id = msg![env; label initWithFrame:bounds];
    let content: id = msg![env; this contentView];
    () = msg![env; content addSubview:label];
    env.objc.borrow_mut::<CellHostObject>(this).text_label = label;
    label
}
- (id)detailTextLabel {
    env.objc.borrow::<CellHostObject>(this).detail_text_label
}
- (())dealloc {
    let host = env.objc.borrow::<CellHostObject>(this);
    let refs = [
        host.reuse_identifier,
        host.content_view,
        host.background_view,
        host.selected_background_view,
        host.text_label,
        host.detail_text_label,
    ];
    for value in refs {
        release(env, value);
    }
    msg_super![env; this dealloc]
}
@end

@implementation UITableViewCellContentView: UIView
@end
};

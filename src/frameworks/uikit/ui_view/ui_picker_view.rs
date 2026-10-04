/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Picker component rows, selection and delegate callbacks.
use super::UIViewHostObject;
use crate::frameworks::core_graphics::{CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::NSInteger;
use crate::objc::{
    id, msg, msg_class, msg_super, nil, objc_classes, release, ClassExports, NSZonePtr,
};
use crate::{impl_HostObject_with_superclass, Environment};
#[derive(Default)]
struct PickerHostObject {
    superclass: UIViewHostObject,
    delegate: id,
    data_source: id,
    selected: Vec<NSInteger>,
    labels: Vec<id>,
    start: Option<CGPoint>,
    indicator: bool,
}
impl_HostObject_with_superclass!(PickerHostObject);
fn responds(env: &mut Environment, object: id, name: &str) -> bool {
    let selector = env.objc.register_host_selector(name.into(), &mut env.mem);
    object != nil && msg![env; object respondsToSelector:selector]
}
fn reload(env: &mut Environment, picker: id) {
    let host = env.objc.borrow_mut::<PickerHostObject>(picker);
    let old = std::mem::take(&mut host.labels);
    let delegate = host.delegate;
    let source = if host.data_source != nil {
        host.data_source
    } else {
        delegate
    };
    for label in old {
        () = msg![env; label removeFromSuperview];
        release(env, label);
    }
    let count: NSInteger = if responds(env, source, "numberOfComponentsInPickerView:") {
        msg![env; source numberOfComponentsInPickerView:picker]
    } else {
        1
    };
    env.objc
        .borrow_mut::<PickerHostObject>(picker)
        .selected
        .resize(count.max(0) as usize, 0);
    let bounds: CGRect = msg![env; picker bounds];
    let width = bounds.size.width / count.max(1) as f32;
    for component in 0..count {
        let rows: NSInteger = msg![env; source pickerView:picker numberOfRowsInComponent:component];
        let selected = env.objc.borrow::<PickerHostObject>(picker).selected[component as usize]
            .clamp(0, (rows - 1).max(0));
        env.objc.borrow_mut::<PickerHostObject>(picker).selected[component as usize] = selected;
        for relative in -2..=2 {
            let row = selected + relative;
            if row < 0 || row >= rows {
                continue;
            }
            let title: id = if responds(env, delegate, "pickerView:titleForRow:forComponent:") {
                msg![env; delegate pickerView:picker titleForRow:row forComponent:component]
            } else {
                nil
            };
            let rect = CGRect {
                origin: CGPoint {
                    x: bounds.origin.x + component as f32 * width,
                    y: bounds.origin.y + bounds.size.height * 0.5 - 18.0 + relative as f32 * 36.0,
                },
                size: CGSize {
                    width,
                    height: 36.0,
                },
            };
            let label: id = msg_class![env; UILabel alloc];
            let label: id = msg![env; label initWithFrame:rect];
            () = msg![env; label setText:title];
            () = msg![env; label setTextAlignment:1i32];
            if relative == 0 {
                let color: id = msg_class![env; UIColor blueColor];
                () = msg![env; label setTextColor:color];
            }
            () = msg![env; picker addSubview:label];
            env.objc
                .borrow_mut::<PickerHostObject>(picker)
                .labels
                .push(label);
        }
    }
    () = msg![env; picker setNeedsDisplay];
}
pub const CLASSES: ClassExports = objc_classes! {
(env, this, _cmd);
@implementation UIPickerView: UIView
+ (id)allocWithZone:(NSZonePtr)_zone {
    env.objc
        .alloc_object(this, Box::<PickerHostObject>::default(), &mut env.mem)
}
- (id)initWithCoder:(id)coder {
    msg_super![env; this initWithCoder:coder]
}
- (())setShowsSelectionIndicator:(bool)value {
    env.objc.borrow_mut::<PickerHostObject>(this).indicator = value;
}
- (())setDelegate:(id)value {
    env.objc.borrow_mut::<PickerHostObject>(this).delegate = value;
}
- (id)delegate {
    env.objc.borrow::<PickerHostObject>(this).delegate
}
- (())setDataSource:(id)value {
    env.objc.borrow_mut::<PickerHostObject>(this).data_source = value;
}
- (id)dataSource {
    env.objc.borrow::<PickerHostObject>(this).data_source
}
- (())reloadAllComponents {
    reload(env, this);
}
- (())reloadComponent:(NSInteger)_component {
    reload(env, this);
}
- (())selectRow:(NSInteger)row inComponent:(NSInteger)component animated:(bool)_animated {
    let host = env.objc.borrow_mut::<PickerHostObject>(this);
    if component < 0 {
        return;
    }
    host.selected
        .resize(host.selected.len().max(component as usize + 1), 0);
    host.selected[component as usize] = row;
    reload(env, this);
}
- (NSInteger)selectedRowInComponent:(NSInteger)component {
    env.objc
        .borrow::<PickerHostObject>(this)
        .selected
        .get(component as usize)
        .copied()
        .unwrap_or(-1)
}
- (())touchesBegan:(id)touches withEvent:(id)_event {
    let touch: id = msg![env; touches anyObject];
    let point: CGPoint = msg![env; touch locationInView:this];
    env.objc.borrow_mut::<PickerHostObject>(this).start = Some(point);
}
- (())touchesEnded:(id)touches withEvent:(id)_event {
    let touch: id = msg![env; touches anyObject];
    let point: CGPoint = msg![env; touch locationInView:this];
    let bounds: CGRect = msg![env; this bounds];
    let host = env.objc.borrow::<PickerHostObject>(this);
    let count = host.selected.len();
    if count == 0 || bounds.size.width <= 0.0 {
        return;
    }
    let component =
        (((point.x - bounds.origin.x) / bounds.size.width * count as f32) as usize).min(count - 1);
    let delta = host.start.map(|start| start.y - point.y).unwrap_or(0.0);
    let row = (host.selected[component] + (delta / 36.0).round() as i32).max(0);
    let delegate = host.delegate;
    () = msg![env; this selectRow:row inComponent:(component as i32) animated:false];
    let selected: i32 = msg![env; this selectedRowInComponent:(component as i32)];
    if responds(env, delegate, "pickerView:didSelectRow:inComponent:") {
        () = msg![env; delegate pickerView:this didSelectRow:selected inComponent:(component as i32)];
    }
}
- (())dealloc {
    let labels = std::mem::take(&mut env.objc.borrow_mut::<PickerHostObject>(this).labels);
    for label in labels {
        release(env, label);
    }
    msg_super![env; this dealloc]
}
@end
};

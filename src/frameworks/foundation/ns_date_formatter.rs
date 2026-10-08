/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSDateFormatter`.
//!
//! Resources:
//! - Apple's [Introduction to Data Formatting Programming Guide For Cocoa](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/DataFormatting/DataFormatting.html)
//! - [Unicode Technical Standard #35](https://unicode.org/reports/tr35/tr35-10.html#Date_Format_Patterns)

use crate::frameworks::core_foundation::time::CFAbsoluteTimeGetGregorianDate;
use crate::frameworks::foundation::{ns_string, NSInteger, NSTimeInterval};
use crate::objc::{
    autorelease, id, msg, nil, objc_classes, release, ClassExports, HostObject, NSZonePtr,
};

struct NSDateFormatterHostObject {
    date_format: Option<id>,
    date_style: NSInteger,
    time_style: NSInteger,
    formatter_behavior: NSInteger,
}
impl HostObject for NSDateFormatterHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSDateFormatter: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(NSDateFormatterHostObject {
        date_format: None,
        date_style: 0,
        time_style: 0,
        formatter_behavior: 0,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (())setFormatterBehavior:(NSInteger)behavior {
    // NSDateFormatterBehaviorDefault (0), 10_0 (1000), and 10_4 (1040)
    // are the public values on the iOS versions touchHLE targets. Keep the
    // value as state even though this formatter currently uses one formatting
    // implementation for all behaviors.
    env.objc
        .borrow_mut::<NSDateFormatterHostObject>(this)
        .formatter_behavior = behavior;
}
- (NSInteger)formatterBehavior {
    env.objc
        .borrow::<NSDateFormatterHostObject>(this)
        .formatter_behavior
}

- (())setDateFormat:(id)format {
    // NSString *
    let date_format: id = msg![env; format copy];
    let old = env
        .objc
        .borrow_mut::<NSDateFormatterHostObject>(this)
        .date_format
        .replace(date_format);
    if let Some(old) = old {
        release(env, old);
    }
}

- (())setDateStyle:(NSInteger)value {
    let host = env.objc.borrow_mut::<NSDateFormatterHostObject>(this);
    host.date_style = value;
    let old = host.date_format.take();
    if let Some(old) = old {
        release(env, old);
    }
}
- (NSInteger)dateStyle {
    env.objc
        .borrow::<NSDateFormatterHostObject>(this)
        .date_style
}
- (())setTimeStyle:(NSInteger)value {
    let host = env.objc.borrow_mut::<NSDateFormatterHostObject>(this);
    host.time_style = value;
    let old = host.date_format.take();
    if let Some(old) = old {
        release(env, old);
    }
}
- (NSInteger)timeStyle {
    env.objc
        .borrow::<NSDateFormatterHostObject>(this)
        .time_style
}
- (())dealloc {
    if let Some(format) = env
        .objc
        .borrow::<NSDateFormatterHostObject>(this)
        .date_format
    {
        release(env, format);
    }
    env.objc.dealloc_object(this, &mut env.mem)
}

- (id)stringFromDate:(id)date {
    let &NSDateFormatterHostObject {
        date_format,
        date_style,
        time_style,
        formatter_behavior: _,
    } = env.objc.borrow(this);
    let mut format = date_format
        .map(|value| ns_string::to_rust_string(env, value).to_string())
        .unwrap_or_default();
    log_dbg!("date_format before: {:?}", format);

    let ti: NSTimeInterval = msg![env; date timeIntervalSinceReferenceDate];
    let greg_date = CFAbsoluteTimeGetGregorianDate(env, ti, nil);
    let year = greg_date.year;
    let month = greg_date.month;
    let day = greg_date.day;
    let hour = greg_date.hours;
    let minute = greg_date.minutes;
    let second = greg_date.seconds;

    if date_format.is_none() {
        let short_months = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let long_months = [
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ];
        let month_index = (month - 1) as usize;
        let date = match date_style {
            0 => String::new(),
            1 => format!("{month}/{day}/{:02}", year % 100),
            2 => format!("{} {day}, {year}", short_months[month_index]),
            _ => format!("{} {day}, {year}", long_months[month_index]),
        };
        let time = match time_style {
            0 => String::new(),
            1 => format!("{hour:02}:{minute:02}"),
            2 => format!("{hour:02}:{minute:02}:{:02}", second as u32),
            _ => format!("{hour:02}:{minute:02}:{:02} GMT", second as u32),
        };
        let result = if date.is_empty() {
            time
        } else if time.is_empty() {
            date
        } else {
            format!("{date}, {time}")
        };
        let result = ns_string::from_rust_string(env, result);
        return autorelease(env, result);
    }

    format = format.replace("yyyy", format!("{year:04}").as_str());
    format = format.replace("YYYY", format!("{year:04}").as_str());
    format = format.replace("MM", format!("{month:02}").as_str());
    format = format.replace("dd", format!("{day:02}").as_str());
    format = format.replace("HH", format!("{hour:02}").as_str());
    format = format.replace("mm", format!("{minute:02}").as_str());
    format = format.replace("ss", format!("{second:02}").as_str());

    for c in format.chars() {
        if let pattern @ ('A'..='Z' | 'a'..='z') = c {
            unimplemented!("date string contains unsubstituted format pattern: {pattern}");
        }
    }
    log_dbg!("date_format after: {:?}", format);

    let res = ns_string::from_rust_string(env, format);
    autorelease(env, res)
}

@end

};

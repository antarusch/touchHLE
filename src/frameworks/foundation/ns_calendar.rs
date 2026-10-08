/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSCalendar` and `NSDateComponents`.
//!
//! touchHLE currently models calendar calculations in UTC. This is consistent
//! with the rest of its date/time implementation and is sufficient for the
//! Gregorian calendar APIs used by many iOS 4-era applications.

use super::{NSInteger, NSUInteger, NSTimeInterval};
use crate::dyld::{ConstantExports, HostConstant};
use crate::libc::time::{timestamp_to_calendar_date, time_t};
use crate::objc::{
    autorelease, id, msg, msg_class, nil, objc_classes, release, retain, ClassExports, HostObject,
    NSZonePtr,
};

const NS_UNDEFINED_DATE_COMPONENT: NSInteger = 0x7fff_ffff;

const NS_ERA_CALENDAR_UNIT: NSUInteger = 1 << 1;
const NS_YEAR_CALENDAR_UNIT: NSUInteger = 1 << 2;
const NS_MONTH_CALENDAR_UNIT: NSUInteger = 1 << 3;
const NS_DAY_CALENDAR_UNIT: NSUInteger = 1 << 4;
const NS_HOUR_CALENDAR_UNIT: NSUInteger = 1 << 5;
const NS_MINUTE_CALENDAR_UNIT: NSUInteger = 1 << 6;
const NS_SECOND_CALENDAR_UNIT: NSUInteger = 1 << 7;
const NS_WEEK_CALENDAR_UNIT: NSUInteger = 1 << 8;
const NS_WEEKDAY_CALENDAR_UNIT: NSUInteger = 1 << 9;
const NS_WEEKDAY_ORDINAL_CALENDAR_UNIT: NSUInteger = 1 << 10;
const NS_QUARTER_CALENDAR_UNIT: NSUInteger = 1 << 11;
const NS_WEEK_OF_MONTH_CALENDAR_UNIT: NSUInteger = 1 << 12;
const NS_WEEK_OF_YEAR_CALENDAR_UNIT: NSUInteger = 1 << 13;
const NS_YEAR_FOR_WEEK_OF_YEAR_CALENDAR_UNIT: NSUInteger = 1 << 14;
const NS_CALENDAR_CALENDAR_UNIT: NSUInteger = 1 << 20;
const NS_TIME_ZONE_CALENDAR_UNIT: NSUInteger = 1 << 21;

pub const CONSTANTS: ConstantExports = &[
    (
        "_NSGregorianCalendar",
        HostConstant::NSString("gregorian"),
    ),
];

#[derive(Default)]
struct NSCalendarHostObject {
    identifier: id,
    time_zone: id,
}
impl HostObject for NSCalendarHostObject {}

#[derive(Clone)]
struct NSDateComponentsHostObject {
    era: NSInteger,
    year: NSInteger,
    month: NSInteger,
    day: NSInteger,
    hour: NSInteger,
    minute: NSInteger,
    second: NSInteger,
    week: NSInteger,
    weekday: NSInteger,
    weekday_ordinal: NSInteger,
    quarter: NSInteger,
    week_of_month: NSInteger,
    week_of_year: NSInteger,
    year_for_week_of_year: NSInteger,
    calendar: id,
    time_zone: id,
}
impl Default for NSDateComponentsHostObject {
    fn default() -> Self {
        Self {
            era: NS_UNDEFINED_DATE_COMPONENT,
            year: NS_UNDEFINED_DATE_COMPONENT,
            month: NS_UNDEFINED_DATE_COMPONENT,
            day: NS_UNDEFINED_DATE_COMPONENT,
            hour: NS_UNDEFINED_DATE_COMPONENT,
            minute: NS_UNDEFINED_DATE_COMPONENT,
            second: NS_UNDEFINED_DATE_COMPONENT,
            week: NS_UNDEFINED_DATE_COMPONENT,
            weekday: NS_UNDEFINED_DATE_COMPONENT,
            weekday_ordinal: NS_UNDEFINED_DATE_COMPONENT,
            quarter: NS_UNDEFINED_DATE_COMPONENT,
            week_of_month: NS_UNDEFINED_DATE_COMPONENT,
            week_of_year: NS_UNDEFINED_DATE_COMPONENT,
            year_for_week_of_year: NS_UNDEFINED_DATE_COMPONENT,
            calendar: nil,
            time_zone: nil,
        }
    }
}
impl HostObject for NSDateComponentsHostObject {}

fn is_leap_year(year: NSInteger) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn day_of_year(year: NSInteger, month: NSInteger, day: NSInteger) -> NSInteger {
    const DAYS_BEFORE_MONTH: [NSInteger; 12] =
        [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let month_index = usize::try_from(month - 1).unwrap();
    DAYS_BEFORE_MONTH[month_index] + day + NSInteger::from(is_leap_year(year) && month > 2)
}

fn set_selected_components_from_date(
    env: &mut crate::Environment,
    components: id,
    units: NSUInteger,
    date: id,
) {
    let seconds: NSTimeInterval = msg![env; date timeIntervalSince1970];
    let timestamp: time_t = seconds as time_t;
    let tm = timestamp_to_calendar_date(timestamp);

    let year = tm.tm_year + 1900;
    let month = tm.tm_mon + 1;
    let day = tm.tm_mday;
    let days_since_epoch = timestamp.div_euclid(86_400);
    // Foundation calendars number Sunday as 1 ... Saturday as 7.
    let weekday = (4 + days_since_epoch).rem_euclid(7) + 1;
    let ordinal = (day - 1) / 7 + 1;
    let week_of_year = (day_of_year(year, month, day) - 1) / 7 + 1;

    let host = env
        .objc
        .borrow_mut::<NSDateComponentsHostObject>(components);
    if units & NS_ERA_CALENDAR_UNIT != 0 {
        host.era = if year >= 1 { 1 } else { 0 };
    }
    if units & NS_YEAR_CALENDAR_UNIT != 0 {
        host.year = year;
    }
    if units & NS_MONTH_CALENDAR_UNIT != 0 {
        host.month = month;
    }
    if units & NS_DAY_CALENDAR_UNIT != 0 {
        host.day = day;
    }
    if units & NS_HOUR_CALENDAR_UNIT != 0 {
        host.hour = tm.tm_hour;
    }
    if units & NS_MINUTE_CALENDAR_UNIT != 0 {
        host.minute = tm.tm_min;
    }
    if units & NS_SECOND_CALENDAR_UNIT != 0 {
        host.second = tm.tm_sec;
    }
    if units & NS_WEEK_CALENDAR_UNIT != 0 {
        host.week = week_of_year;
    }
    if units & NS_WEEKDAY_CALENDAR_UNIT != 0 {
        host.weekday = weekday;
    }
    if units & NS_WEEKDAY_ORDINAL_CALENDAR_UNIT != 0 {
        host.weekday_ordinal = ordinal;
    }
    if units & NS_QUARTER_CALENDAR_UNIT != 0 {
        host.quarter = (month - 1) / 3 + 1;
    }
    if units & NS_WEEK_OF_MONTH_CALENDAR_UNIT != 0 {
        host.week_of_month = ordinal;
    }
    if units & NS_WEEK_OF_YEAR_CALENDAR_UNIT != 0 {
        host.week_of_year = week_of_year;
    }
    if units & NS_YEAR_FOR_WEEK_OF_YEAR_CALENDAR_UNIT != 0 {
        host.year_for_week_of_year = year;
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSCalendar: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<NSCalendarHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

+ (id)currentCalendar {
    let identifier = super::ns_string::get_static_str(env, "gregorian");
    let calendar: id = msg![env; this alloc];
    let calendar: id = msg![env; calendar initWithCalendarIdentifier:identifier];
    autorelease(env, calendar)
}

+ (id)calendarWithIdentifier:(id)identifier {
    let calendar: id = msg![env; this alloc];
    let calendar: id = msg![env; calendar initWithCalendarIdentifier:identifier];
    autorelease(env, calendar)
}

- (id)initWithCalendarIdentifier:(id)identifier {
    let identifier = if identifier == nil {
        super::ns_string::get_static_str(env, "gregorian")
    } else {
        identifier
    };
    let copied: id = msg![env; identifier copy];
    env.objc.borrow_mut::<NSCalendarHostObject>(this).identifier = copied;
    this
}

- (())dealloc {
    let (identifier, time_zone) = {
        let host = env.objc.borrow::<NSCalendarHostObject>(this);
        (host.identifier, host.time_zone)
    };
    release(env, identifier);
    release(env, time_zone);
    env.objc.dealloc_object(this, &mut env.mem)
}

- (id)calendarIdentifier {
    env.objc.borrow::<NSCalendarHostObject>(this).identifier
}

- (id)timeZone {
    let time_zone = env.objc.borrow::<NSCalendarHostObject>(this).time_zone;
    if time_zone == nil {
        msg_class![env; NSTimeZone defaultTimeZone]
    } else {
        time_zone
    }
}

- (())setTimeZone:(id)time_zone {
    let new_time_zone = retain(env, time_zone);
    let old_time_zone = env.objc.borrow::<NSCalendarHostObject>(this).time_zone;
    env.objc.borrow_mut::<NSCalendarHostObject>(this).time_zone = new_time_zone;
    release(env, old_time_zone);
}

- (id)components:(NSUInteger)units fromDate:(id)date {
    if date == nil {
        return nil;
    }
    let components: id = msg_class![env; NSDateComponents new];
    set_selected_components_from_date(env, components, units, date);

    let host = env.objc.borrow_mut::<NSDateComponentsHostObject>(components);
    if units & NS_CALENDAR_CALENDAR_UNIT != 0 {
        host.calendar = retain(env, this);
    }
    if units & NS_TIME_ZONE_CALENDAR_UNIT != 0 {
        let time_zone: id = msg![env; this timeZone];
        host.time_zone = retain(env, time_zone);
    }

    autorelease(env, components)
}

- (id)components:(NSUInteger)units
        fromDate:(id)from_date
          toDate:(id)to_date
         options:(NSUInteger)_options {
    if from_date == nil || to_date == nil {
        return nil;
    }

    let from_seconds: NSTimeInterval = msg![env; from_date timeIntervalSince1970];
    let to_seconds: NSTimeInterval = msg![env; to_date timeIntervalSince1970];
    let mut remaining = (to_seconds - from_seconds) as NSInteger;
    let sign = if remaining < 0 { -1 } else { 1 };
    remaining = remaining.abs();

    let components: id = msg_class![env; NSDateComponents new];
    let host = env.objc.borrow_mut::<NSDateComponentsHostObject>(components);

    if units & NS_YEAR_CALENDAR_UNIT != 0 {
        host.year = sign * (remaining / (365 * 86_400));
        remaining %= 365 * 86_400;
    }
    if units & NS_MONTH_CALENDAR_UNIT != 0 {
        host.month = sign * (remaining / (30 * 86_400));
        remaining %= 30 * 86_400;
    }
    if units & NS_WEEK_CALENDAR_UNIT != 0 {
        host.week = sign * (remaining / (7 * 86_400));
        remaining %= 7 * 86_400;
    }
    if units & NS_DAY_CALENDAR_UNIT != 0 {
        host.day = sign * (remaining / 86_400);
        remaining %= 86_400;
    }
    if units & NS_HOUR_CALENDAR_UNIT != 0 {
        host.hour = sign * (remaining / 3_600);
        remaining %= 3_600;
    }
    if units & NS_MINUTE_CALENDAR_UNIT != 0 {
        host.minute = sign * (remaining / 60);
        remaining %= 60;
    }
    if units & NS_SECOND_CALENDAR_UNIT != 0 {
        host.second = sign * remaining;
    }

    autorelease(env, components)
}

- (id)dateFromComponents:(id)components {
    if components == nil {
        return nil;
    }
    let host = env.objc.borrow::<NSDateComponentsHostObject>(components);
    if host.year == NS_UNDEFINED_DATE_COMPONENT
        || !(1900..=2038).contains(&host.year)
    {
        log!(
            "Warning: NSCalendar dateFromComponents: cannot represent year {} in 32-bit time_t",
            host.year
        );
        return nil;
    }

    let month = if host.month == NS_UNDEFINED_DATE_COMPONENT {
        1
    } else {
        host.month
    };
    let day = if host.day == NS_UNDEFINED_DATE_COMPONENT {
        1
    } else {
        host.day
    };
    let hour = if host.hour == NS_UNDEFINED_DATE_COMPONENT {
        0
    } else {
        host.hour
    };
    let minute = if host.minute == NS_UNDEFINED_DATE_COMPONENT {
        0
    } else {
        host.minute
    };
    let second = if host.second == NS_UNDEFINED_DATE_COMPONENT {
        0
    } else {
        host.second
    };

    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=60).contains(&second)
    {
        return nil;
    }

    // Convert Gregorian UTC components directly to a Unix timestamp. This
    // mirrors touchHLE's existing UTC-only Foundation time model.
    let mut days: i64 = 0;
    for year in 1970..host.year {
        days += if is_leap_year(year) { 366 } else { 365 };
    }
    if host.year < 1970 {
        for year in host.year..1970 {
            days -= if is_leap_year(year) { 366 } else { 365 };
        }
    }
    days += i64::from(day_of_year(host.year, month, day) - 1);

    let timestamp = days * 86_400
        + i64::from(hour) * 3_600
        + i64::from(minute) * 60
        + i64::from(second);
    let timestamp: NSTimeInterval = timestamp as NSTimeInterval;
    msg_class![env; NSDate dateWithTimeIntervalSince1970:timestamp]
}

@end

@implementation NSDateComponents: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<NSDateComponentsHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (())dealloc {
    let (calendar, time_zone) = {
        let host = env.objc.borrow::<NSDateComponentsHostObject>(this);
        (host.calendar, host.time_zone)
    };
    release(env, calendar);
    release(env, time_zone);
    env.objc.dealloc_object(this, &mut env.mem)
}

- (NSInteger)era { env.objc.borrow::<NSDateComponentsHostObject>(this).era }
- (())setEra:(NSInteger)value { env.objc.borrow_mut::<NSDateComponentsHostObject>(this).era = value; }
- (NSInteger)year { env.objc.borrow::<NSDateComponentsHostObject>(this).year }
- (())setYear:(NSInteger)value { env.objc.borrow_mut::<NSDateComponentsHostObject>(this).year = value; }
- (NSInteger)month { env.objc.borrow::<NSDateComponentsHostObject>(this).month }
- (())setMonth:(NSInteger)value { env.objc.borrow_mut::<NSDateComponentsHostObject>(this).month = value; }
- (NSInteger)day { env.objc.borrow::<NSDateComponentsHostObject>(this).day }
- (())setDay:(NSInteger)value { env.objc.borrow_mut::<NSDateComponentsHostObject>(this).day = value; }
- (NSInteger)hour { env.objc.borrow::<NSDateComponentsHostObject>(this).hour }
- (())setHour:(NSInteger)value { env.objc.borrow_mut::<NSDateComponentsHostObject>(this).hour = value; }
- (NSInteger)minute { env.objc.borrow::<NSDateComponentsHostObject>(this).minute }
- (())setMinute:(NSInteger)value { env.objc.borrow_mut::<NSDateComponentsHostObject>(this).minute = value; }
- (NSInteger)second { env.objc.borrow::<NSDateComponentsHostObject>(this).second }
- (())setSecond:(NSInteger)value { env.objc.borrow_mut::<NSDateComponentsHostObject>(this).second = value; }
- (NSInteger)week { env.objc.borrow::<NSDateComponentsHostObject>(this).week }
- (())setWeek:(NSInteger)value { env.objc.borrow_mut::<NSDateComponentsHostObject>(this).week = value; }
- (NSInteger)weekday { env.objc.borrow::<NSDateComponentsHostObject>(this).weekday }
- (())setWeekday:(NSInteger)value { env.objc.borrow_mut::<NSDateComponentsHostObject>(this).weekday = value; }
- (NSInteger)weekdayOrdinal { env.objc.borrow::<NSDateComponentsHostObject>(this).weekday_ordinal }
- (())setWeekdayOrdinal:(NSInteger)value { env.objc.borrow_mut::<NSDateComponentsHostObject>(this).weekday_ordinal = value; }
- (NSInteger)quarter { env.objc.borrow::<NSDateComponentsHostObject>(this).quarter }
- (())setQuarter:(NSInteger)value { env.objc.borrow_mut::<NSDateComponentsHostObject>(this).quarter = value; }
- (NSInteger)weekOfMonth { env.objc.borrow::<NSDateComponentsHostObject>(this).week_of_month }
- (())setWeekOfMonth:(NSInteger)value { env.objc.borrow_mut::<NSDateComponentsHostObject>(this).week_of_month = value; }
- (NSInteger)weekOfYear { env.objc.borrow::<NSDateComponentsHostObject>(this).week_of_year }
- (())setWeekOfYear:(NSInteger)value { env.objc.borrow_mut::<NSDateComponentsHostObject>(this).week_of_year = value; }
- (NSInteger)yearForWeekOfYear { env.objc.borrow::<NSDateComponentsHostObject>(this).year_for_week_of_year }
- (())setYearForWeekOfYear:(NSInteger)value { env.objc.borrow_mut::<NSDateComponentsHostObject>(this).year_for_week_of_year = value; }

- (id)calendar { env.objc.borrow::<NSDateComponentsHostObject>(this).calendar }
- (())setCalendar:(id)calendar {
    let new_calendar = retain(env, calendar);
    let old_calendar = env.objc.borrow::<NSDateComponentsHostObject>(this).calendar;
    env.objc.borrow_mut::<NSDateComponentsHostObject>(this).calendar = new_calendar;
    release(env, old_calendar);
}

- (id)timeZone { env.objc.borrow::<NSDateComponentsHostObject>(this).time_zone }
- (())setTimeZone:(id)time_zone {
    let new_time_zone = retain(env, time_zone);
    let old_time_zone = env.objc.borrow::<NSDateComponentsHostObject>(this).time_zone;
    env.objc.borrow_mut::<NSDateComponentsHostObject>(this).time_zone = new_time_zone;
    release(env, old_time_zone);
}

@end

};

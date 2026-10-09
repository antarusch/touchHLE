/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSNumberFormatter`.

use super::{ns_string, NSInteger, NSUInteger};
use crate::objc::{
    autorelease, id, msg, msg_class, nil, objc_classes, release, ClassExports, HostObject, NSZonePtr,
};

struct NSNumberFormatterHostObject {
    number_style: NSInteger,
    uses_grouping_separator: bool,
    grouping_separator: Option<id>,
    grouping_size: NSUInteger,
}
impl HostObject for NSNumberFormatterHostObject {}

fn apply_grouping(input: &str, separator: &str, grouping_size: usize) -> String {
    if separator.is_empty() || grouping_size == 0 {
        return input.to_string();
    }

    let (mantissa, exponent) = match input.find(['e', 'E']) {
        Some(index) => (&input[..index], &input[index..]),
        None => (input, ""),
    };
    let (integer, fraction) = match mantissa.find('.') {
        Some(index) => (&mantissa[..index], &mantissa[index..]),
        None => (mantissa, ""),
    };

    let (sign, digits) = if let Some(digits) = integer.strip_prefix('-') {
        ("-", digits)
    } else if let Some(digits) = integer.strip_prefix('+') {
        ("+", digits)
    } else {
        ("", integer)
    };

    if digits.len() <= grouping_size || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return input.to_string();
    }

    let first_group = match digits.len() % grouping_size {
        0 => grouping_size,
        remainder => remainder,
    };
    let mut result = String::with_capacity(input.len() + digits.len() / 3 * separator.len());
    result.push_str(sign);
    result.push_str(&digits[..first_group]);
    for chunk in digits.as_bytes()[first_group..].chunks(grouping_size) {
        result.push_str(separator);
        result.push_str(std::str::from_utf8(chunk).unwrap());
    }
    result.push_str(fraction);
    result.push_str(exponent);
    result
}

// NSNumberFormatter returns nil if the input does not represent a number.
// Keep integer values as integers, rather than losing precision through f64.
#[derive(Debug, PartialEq)]
enum ParsedNumber {
    Signed(i64),
    Unsigned(u64),
    Double(f64),
}

fn parse_number(
    input: &str,
    grouping_separator: Option<&str>,
    grouping_size: usize,
) -> Option<ParsedNumber> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    let normalized = if let Some(separator) = grouping_separator.filter(|s| !s.is_empty()) {
        if input.contains(separator) {
            if grouping_size == 0 {
                return None;
            }
            let exponent_at = input.find(['e', 'E']).unwrap_or(input.len());
            let (mantissa, exponent) = input.split_at(exponent_at);
            let integer_end = if separator == "." {
                mantissa.len()
            } else {
                mantissa.find('.').unwrap_or(mantissa.len())
            };
            let (integer, fractional) = mantissa.split_at(integer_end);
            let (sign, digits) = if let Some(digits) = integer.strip_prefix('-') {
                ("-", digits)
            } else if let Some(digits) = integer.strip_prefix('+') {
                ("+", digits)
            } else {
                ("", integer)
            };
            let groups: Vec<_> = digits.split(separator).collect();
            if groups.len() < 2
                || groups[0].is_empty()
                || groups[0].len() > grouping_size
                || !groups[0].bytes().all(|c| c.is_ascii_digit())
                || groups[1..].iter().any(|group| {
                    group.len() != grouping_size
                        || !group.bytes().all(|c| c.is_ascii_digit())
                })
            {
                return None;
            }
            format!("{}{}{}{}", sign, groups.concat(), fractional, exponent)
        } else {
            input.to_string()
        }
    } else {
        input.to_string()
    };

    if let Ok(value) = normalized.parse::<i64>() {
        return Some(ParsedNumber::Signed(value));
    }
    if let Ok(value) = normalized.parse::<u64>() {
        return Some(ParsedNumber::Unsigned(value));
    }
    let value = normalized.parse::<f64>().ok()?;
    value.is_finite().then_some(ParsedNumber::Double(value))
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSNumberFormatter: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(NSNumberFormatterHostObject {
        number_style: 0,
        uses_grouping_separator: false,
        grouping_separator: None,
        grouping_size: 3,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (())setLocale:(id)_locale {
    // Rogue Planet sets the formatter locale to match its selected language.
    // The current formatter implementation is locale-neutral apart from the
    // explicitly configured grouping separator.
}

- (())setFormatterBehavior:(NSInteger)_behavior {
    // The game requests an older formatter behavior mode. The supported
    // formatting operations below do not differ between those modes.
}

- (())setNumberStyle:(NSInteger)style {
    env.objc
        .borrow_mut::<NSNumberFormatterHostObject>(this)
        .number_style = style;
}

- (NSInteger)numberStyle {
    env.objc
        .borrow::<NSNumberFormatterHostObject>(this)
        .number_style
}

- (())setUsesGroupingSeparator:(bool)uses_grouping_separator {
    env.objc
        .borrow_mut::<NSNumberFormatterHostObject>(this)
        .uses_grouping_separator = uses_grouping_separator;
}

- (bool)usesGroupingSeparator {
    env.objc
        .borrow::<NSNumberFormatterHostObject>(this)
        .uses_grouping_separator
}

- (())setGroupingSeparator:(id)separator {
    let copied: id = msg![env; separator copy];
    let old = env
        .objc
        .borrow_mut::<NSNumberFormatterHostObject>(this)
        .grouping_separator
        .replace(copied);
    if let Some(old) = old {
        release(env, old);
    }
}

- (id)groupingSeparator {
    let separator = env
        .objc
        .borrow::<NSNumberFormatterHostObject>(this)
        .grouping_separator;
    separator.unwrap_or_else(|| ns_string::get_static_str(env, ","))
}

- (())setGroupingSize:(NSUInteger)grouping_size {
    env.objc
        .borrow_mut::<NSNumberFormatterHostObject>(this)
        .grouping_size = grouping_size;
}

- (NSUInteger)groupingSize {
    env.objc
        .borrow::<NSNumberFormatterHostObject>(this)
        .grouping_size
}

- (id)numberFromString:(id)string {
    if string == nil {
        return nil;
    }

    let input = ns_string::to_rust_string(env, string).into_owned();
    let (grouping_enabled, grouping_separator, grouping_size) = {
        let host = env.objc.borrow::<NSNumberFormatterHostObject>(this);
        (
            host.uses_grouping_separator,
            host.grouping_separator,
            host.grouping_size as usize,
        )
    };
    let separator = if grouping_enabled {
        Some(
            grouping_separator
                .map(|value| ns_string::to_rust_string(env, value).into_owned())
                .unwrap_or_else(|| ",".to_string()),
        )
    } else {
        None
    };

    match parse_number(&input, separator.as_deref(), grouping_size) {
        Some(ParsedNumber::Signed(value)) => {
            msg_class![env; NSNumber numberWithLongLong:value]
        }
        Some(ParsedNumber::Unsigned(value)) => {
            msg_class![env; NSNumber numberWithUnsignedLongLong:value]
        }
        Some(ParsedNumber::Double(value)) => {
            msg_class![env; NSNumber numberWithDouble:value]
        }
        None => nil,
    }
}

- (id)stringFromNumber:(id)number {
    if number == nil {
        return nil;
    }

    let string: id = msg![env; number stringValue];
    let mut result = ns_string::to_rust_string(env, string).into_owned();
    let (uses_grouping_separator, number_style, grouping_separator, grouping_size) = {
        let host = env.objc.borrow::<NSNumberFormatterHostObject>(this);
        (
            host.uses_grouping_separator,
            host.number_style,
            host.grouping_separator,
            host.grouping_size,
        )
    };

    // NSNumberFormatterDecimalStyle is 1. Rogue Planet also explicitly
    // controls grouping, so apply grouping for plain/decimal styles only.
    if uses_grouping_separator && matches!(number_style, 0 | 1) {
        let separator = grouping_separator
            .map(|value| ns_string::to_rust_string(env, value).into_owned())
            .unwrap_or_else(|| ",".to_string());
        result = apply_grouping(&result, &separator, grouping_size as usize);
    }

    let result = ns_string::from_rust_string(env, result);
    autorelease(env, result)
}

- (())dealloc {
    if let Some(separator) = env
        .objc
        .borrow::<NSNumberFormatterHostObject>(this)
        .grouping_separator
    {
        release(env, separator);
    }
    env.objc.dealloc_object(this, &mut env.mem)
}

@end

};

#[cfg(test)]
mod tests {
    use super::apply_grouping;

    #[test]
    fn parse_plain_numbers_and_reject_invalid_input() {
        use super::ParsedNumber;
        let parse = |text| super::parse_number(text, None, 3);
        assert_eq!(parse("2000"), Some(ParsedNumber::Signed(2000)));
        assert_eq!(parse(" -42 "), Some(ParsedNumber::Signed(-42)));
        assert_eq!(parse("1.25"), Some(ParsedNumber::Double(1.25)));
        assert_eq!(parse("1e3"), Some(ParsedNumber::Double(1000.0)));
        assert_eq!(
            parse("18446744073709551615"),
            Some(ParsedNumber::Unsigned(u64::MAX))
        );
        assert_eq!(parse(""), None);
        assert_eq!(parse("no value"), None);
        assert_eq!(parse("2,000"), None);
        assert_eq!(parse("NaN"), None);
        assert_eq!(parse("Infinity"), None);
    }

    #[test]
    fn parse_grouped_numbers_when_enabled() {
        use super::ParsedNumber;
        let parse = |text| super::parse_number(text, Some(","), 3);
        assert_eq!(parse("2,000"), Some(ParsedNumber::Signed(2000)));
        assert_eq!(parse("-1,234.5"), Some(ParsedNumber::Double(-1234.5)));
        assert_eq!(parse("1,234,567"), Some(ParsedNumber::Signed(1234567)));
        assert_eq!(parse("12,34"), None);
        assert_eq!(parse("1,,234"), None);
        assert_eq!(parse("1234,567"), None);
    }

    #[test]
    fn grouping_keeps_sign_fraction_and_exponent() {
        assert_eq!(apply_grouping("123", ",", 3), "123");
        assert_eq!(apply_grouping("1234", ",", 3), "1,234");
        assert_eq!(apply_grouping("-1234567.5", " ", 3), "-1 234 567.5");
        assert_eq!(apply_grouping("1234567e+10", ".", 3), "1.234.567e+10");
        assert_eq!(apply_grouping("12345678", ",", 4), "1234,5678");
        assert_eq!(apply_grouping("12345678", ",", 0), "12345678");
    }
}

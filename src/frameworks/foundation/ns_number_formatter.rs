/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSNumberFormatter`.

use super::{ns_string, NSInteger};
use crate::objc::{
    autorelease, id, msg, nil, objc_classes, release, ClassExports, HostObject, NSZonePtr,
};

struct NSNumberFormatterHostObject {
    number_style: NSInteger,
    uses_grouping_separator: bool,
    grouping_separator: Option<id>,
}
impl HostObject for NSNumberFormatterHostObject {}

fn apply_grouping(input: &str, separator: &str) -> String {
    if separator.is_empty() {
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

    if digits.len() <= 3 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return input.to_string();
    }

    let first_group = match digits.len() % 3 {
        0 => 3,
        remainder => remainder,
    };
    let mut result = String::with_capacity(input.len() + digits.len() / 3 * separator.len());
    result.push_str(sign);
    result.push_str(&digits[..first_group]);
    for chunk in digits[first_group..].as_bytes().chunks(3) {
        result.push_str(separator);
        result.push_str(std::str::from_utf8(chunk).unwrap());
    }
    result.push_str(fraction);
    result.push_str(exponent);
    result
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSNumberFormatter: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(NSNumberFormatterHostObject {
        number_style: 0,
        uses_grouping_separator: false,
        grouping_separator: None,
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

- (id)stringFromNumber:(id)number {
    if number == nil {
        return nil;
    }

    let string: id = msg![env; number stringValue];
    let mut result = ns_string::to_rust_string(env, string).into_owned();
    let (uses_grouping_separator, number_style, grouping_separator) = {
        let host = env.objc.borrow::<NSNumberFormatterHostObject>(this);
        (
            host.uses_grouping_separator,
            host.number_style,
            host.grouping_separator,
        )
    };

    // NSNumberFormatterDecimalStyle is 1. Rogue Planet also explicitly
    // controls grouping, so apply grouping for plain/decimal styles only.
    if uses_grouping_separator && matches!(number_style, 0 | 1) {
        let separator = grouping_separator
            .map(|value| ns_string::to_rust_string(env, value).into_owned())
            .unwrap_or_else(|| ",".to_string());
        result = apply_grouping(&result, &separator);
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
    fn grouping_keeps_sign_fraction_and_exponent() {
        assert_eq!(apply_grouping("123", ","), "123");
        assert_eq!(apply_grouping("1234", ","), "1,234");
        assert_eq!(apply_grouping("-1234567.5", " "), "-1 234 567.5");
        assert_eq!(apply_grouping("1234567e+10", "."), "1.234.567e+10");
    }
}

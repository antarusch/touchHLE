/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSLocale`.

use super::{ns_array, ns_string};
use crate::dyld::{ConstantExports, HostConstant};
use crate::frameworks::core_foundation::cf_locale::{
    kCFLocaleCountryCode, kCFLocaleIdentifier, kCFLocaleLanguageCode,
};
use crate::objc::{
    autorelease, id, msg, nil, objc_classes, release, retain, ClassExports, HostObject, NSZonePtr,
};
use crate::window::{get_preferred_country_codes, get_preferred_language_codes};
use crate::Environment;

const NSLocaleCountryCode: &str = "NSLocaleCountryCode";
const NSLocaleLanguageCode: &str = "NSLocaleLanguageCode";
const NSLocaleIdentifier: &str = "NSLocaleIdentifier";

pub const CONSTANTS: ConstantExports = &[
    (
        "_NSLocaleCountryCode",
        HostConstant::NSString(NSLocaleCountryCode),
    ),
    (
        "_NSLocaleLanguageCode",
        HostConstant::NSString(NSLocaleLanguageCode),
    ),
    (
        "_NSLocaleIdentifier",
        HostConstant::NSString(NSLocaleIdentifier),
    ),
];

#[derive(Default)]
pub struct State {
    current_locale: Option<id>,
    system_locale: Option<id>,
    preferred_languages: Option<id>,
}
impl State {
    fn get(env: &mut Environment) -> &mut State {
        &mut env.framework_state.foundation.ns_locale
    }
}

/// Use `msg_class![env; NSLocale preferredLanguages]` rather than calling this
/// directly, because it may be slow and there is no caching.
fn get_preferred_languages(env: &mut Environment) -> Vec<String> {
    let options = env.options.as_ref();
    if let Some(ref preferred_languages) = options.preferred_languages {
        log!("The app requested your preferred languages. {:?} will reported based on your --preferred-languages= option.", preferred_languages);
        return preferred_languages.clone();
    }

    let languages = get_preferred_language_codes(env);
    if languages.is_empty() {
        let lang = "en".to_string();
        log!("The app requested your preferred languages. No information could be retrieved, so {:?} (English) will be reported.", lang);
        vec![lang]
    } else {
        log!("The app requested your preferred languages. {:?} will be reported based on your system language preferences.", languages);
        languages
    }
}

fn get_preferred_countries(env: &mut Environment) -> Vec<String> {
    let countries = get_preferred_country_codes(env);
    if countries.is_empty() {
        let country = "US".to_string();
        log!("The app requested your current locale. No country information could be retrieved, so {:?} will be reported.", country);
        vec![country]
    } else {
        log!("The app requested your current locale. {:?} will be reported based on your system region settings.", countries);
        countries
    }
}

fn parse_locale_identifier(identifier: &str) -> (String, Option<String>) {
    let mut components = identifier.split(|c| c == '_' || c == '-');
    let language = components.next().unwrap_or_default().to_ascii_lowercase();
    let country = components.find_map(|component| {
        let ascii_alpha_region =
            component.len() == 2 && component.bytes().all(|byte| byte.is_ascii_alphabetic());
        let numeric_region =
            component.len() == 3 && component.bytes().all(|byte| byte.is_ascii_digit());
        if ascii_alpha_region {
            Some(component.to_ascii_uppercase())
        } else if numeric_region {
            Some(component.to_string())
        } else {
            None
        }
    });
    (language, country)
}

struct NSLocaleHostObject {
    /// `NSString *`
    country_code: id,
    /// `NSString *`
    language_code: id,
}
impl HostObject for NSLocaleHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSLocale: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(NSLocaleHostObject {
        country_code: nil,
        language_code: nil,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

// The documentation isn't clear about what the format of the strings should be,
// but Super Monkey Ball does `isEqualToString:` against "fr", "es", "de", "it"
// and "ja", and its locale detection works properly, so presumably they do not
// usually have region suffixes.
+ (id)preferredLanguages {
    if let Some(existing) = State::get(env).preferred_languages {
        existing
    } else {
        let langs = get_preferred_languages(env);
        let lang_ns_strings = langs.into_iter().map(|lang| ns_string::from_rust_string(env, lang)).collect();
        let new = ns_array::from_vec(env, lang_ns_strings);
        State::get(env).preferred_languages = Some(new);
        new
    }
}

+ (id)currentLocale {
    if let Some(locale) = State::get(env).current_locale {
        locale
    } else {
        let countries = get_preferred_countries(env);
        let country_code = ns_string::from_rust_string(env, countries[0].clone());
        let languages = get_preferred_languages(env);
        let language_code = ns_string::from_rust_string(env, languages[0].clone());
        let host_object = NSLocaleHostObject {
            country_code,
            language_code,
        };
        let new_locale = env.objc.alloc_object(
            this,
            Box::new(host_object),
            &mut env.mem
        );
        State::get(env).current_locale = Some(new_locale);
        new_locale
    }
}
+ (id)autoupdatingCurrentLocale {
    // TODO: autoupdating part
    msg![env; this currentLocale]
}

+ (id)systemLocale {
    if let Some(locale) = State::get(env).system_locale {
        locale
    } else {
        let host_object = NSLocaleHostObject {
            // Was confirmed on the iOS Simulator
            country_code: nil,
            language_code: nil,
        };
        let new_locale = env.objc.alloc_object(
            this,
            Box::new(host_object),
            &mut env.mem
        );
        State::get(env).system_locale = Some(new_locale);
        new_locale
    }
}

// TODO: constructors, more accessors

- (id)initWithLocaleIdentifier:(id)string { // NSString *
    let str = ns_string::to_rust_string(env, string);
    log_dbg!("[(NSLocale *){:?} initWithLocaleIdentifier:'{}']", this, str);
    let (language, country) = parse_locale_identifier(&str);
    let language_code = ns_string::from_rust_string(env, language);
    let country_code = country
        .map(|country| ns_string::from_rust_string(env, country))
        .unwrap_or(nil);
    let host_object: &mut NSLocaleHostObject = env.objc.borrow_mut(this);
    assert!(host_object.language_code == nil);
    assert!(host_object.country_code == nil);
    host_object.language_code = language_code;
    host_object.country_code = country_code;
    this
}

- (())dealloc {
    let &NSLocaleHostObject { country_code, language_code } = env.objc.borrow::<NSLocaleHostObject>(this);
    release(env, country_code);
    release(env, language_code);
    env.objc.dealloc_object(this, &mut env.mem)
}

// NSCopying implementation
- (id)copyWithZone:(NSZonePtr)_zone {
    retain(env, this)
}

- (id)localeIdentifier {
    let locale_id_key = ns_string::get_static_str(env, NSLocaleIdentifier);
    msg![env; this objectForKey:locale_id_key]
}

- (id)objectForKey:(id)key {
    let key_str: &str = &ns_string::to_rust_string(env, key);
    match key_str {
        // Note: this is not the cleanest separation between NS and CF parts
        // But it does work on the iOS Simulator
        // TODO: Define NSLocaleCountryCode _as_ kCFLocaleCountryCode
        NSLocaleCountryCode | kCFLocaleCountryCode => {
            let &NSLocaleHostObject { country_code, .. } = env.objc.borrow(this);
            country_code
        },
        // TODO: Define NSLocaleIdentifier _as_ kCFLocaleIdentifier
        NSLocaleIdentifier | kCFLocaleIdentifier => {
            let (country_code, language_code) = {
                let host_object: &NSLocaleHostObject = env.objc.borrow(this);
                (host_object.country_code, host_object.language_code)
            };
            if language_code == nil {
                nil
            } else {
                let language = ns_string::to_rust_string(env, language_code).into_owned();
                let locale_id_str = if country_code == nil {
                    language
                } else {
                    let country = ns_string::to_rust_string(env, country_code).into_owned();
                    format!("{}_{}", language, country)
                };
                let res = ns_string::from_rust_string(env, locale_id_str);
                autorelease(env, res)
            }
        },
        NSLocaleLanguageCode | kCFLocaleLanguageCode => {
            let &NSLocaleHostObject { language_code, .. } = env.objc.borrow(this);
            language_code
        },
        _ => unimplemented!()
    }
}

@end

};

#[cfg(test)]
mod tests {
    use super::parse_locale_identifier;

    #[test]
    fn locale_identifier_parsing() {
        assert_eq!(parse_locale_identifier("en"), ("en".to_string(), None));
        assert_eq!(
            parse_locale_identifier("en_US"),
            ("en".to_string(), Some("US".to_string()))
        );
        assert_eq!(
            parse_locale_identifier("pt-BR"),
            ("pt".to_string(), Some("BR".to_string()))
        );
        assert_eq!(
            parse_locale_identifier("zh_Hant_CN"),
            ("zh".to_string(), Some("CN".to_string()))
        );
        assert_eq!(
            parse_locale_identifier("es_419"),
            ("es".to_string(), Some("419".to_string()))
        );
    }
}

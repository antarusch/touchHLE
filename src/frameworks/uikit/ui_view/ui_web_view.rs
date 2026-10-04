/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Basic static HTML text display for About panels. No script engine.
use super::UIViewHostObject;
use crate::frameworks::core_graphics::{CGRect, CGSize};
use crate::frameworks::foundation::ns_string::{from_rust_string, to_rust_string};
use crate::impl_HostObject_with_superclass;
use crate::objc::{
    id, msg, msg_class, msg_super, nil, objc_classes, release, retain, ClassExports, NSZonePtr,
};
#[derive(Default)]
struct WebViewHostObject {
    superclass: UIViewHostObject,
    delegate: id,
    html: id,
    base_url: id,
    scroll: id,
}
impl_HostObject_with_superclass!(WebViewHostObject);
fn plain_text(html: &str) -> String {
    let mut text = String::new();
    let mut tag = String::new();
    let mut in_tag = false;
    let mut suppressed: Option<String> = None;
    for c in html.chars() {
        if c == '<' {
            in_tag = true;
            tag.clear();
        } else if in_tag && c == '>' {
            in_tag = false;
            let name = tag
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            if let Some(open) = &suppressed {
                if name == format!("/{open}") {
                    suppressed = None;
                }
                continue;
            }
            if matches!(name.as_str(), "head" | "style" | "script") {
                suppressed = Some(name);
                continue;
            }
            if matches!(
                name.as_str(),
                "br" | "br/" | "/p" | "/div" | "/h1" | "/h2" | "/h3" | "/li" | "/tr"
            ) {
                text.push('\n');
            }
        } else if in_tag {
            tag.push(c);
        } else if suppressed.is_none() {
            text.push(c);
        }
    }
    text.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

#[cfg(test)]
mod tests {
    use super::plain_text;
    #[test]
    fn about_text_excludes_styles_and_scripts() {
        let html = "<head><style>body{color:white}</style></head><body><h2>About</h2><p>A &amp; B<br>Version 1</p><script>hidden()</script></body>";
        assert_eq!(plain_text(html), "About\nA & B\nVersion 1\n");
    }
}
pub const CLASSES: ClassExports = objc_classes! {
(env, this, _cmd);
@implementation UIWebView: UIView
+ (id)allocWithZone:(NSZonePtr)_zone {
    env.objc
        .alloc_object(this, Box::<WebViewHostObject>::default(), &mut env.mem)
}
- (id)initWithCoder:(id)coder {
    msg_super![env; this initWithCoder:coder]
}
- (())setScalesPageToFit:(bool)_scales {}
- (())setDelegate:(id)delegate {
    env.objc.borrow_mut::<WebViewHostObject>(this).delegate = delegate;
}
- (id)delegate {
    env.objc.borrow::<WebViewHostObject>(this).delegate
}
- (())loadHTMLString:(id)html baseURL:(id)url {
    let html_text = to_rust_string(env, html);
    let white_text = html_text.contains("color:white");
    let text = plain_text(&html_text);
    retain(env, html);
    retain(env, url);
    let host = env.objc.borrow_mut::<WebViewHostObject>(this);
    let old = [host.html, host.base_url, host.scroll];
    host.html = html;
    host.base_url = url;
    host.scroll = nil;
    let delegate = host.delegate;
    let sel = env
        .objc
        .register_host_selector("webViewDidStartLoad:".into(), &mut env.mem);
    if delegate != nil && msg![env; delegate respondsToSelector:sel] {
        () = msg![env; delegate webViewDidStartLoad:this];
    }
    () = msg![env; (old[2]) removeFromSuperview];
    for value in old {
        release(env, value);
    }
    let bounds: CGRect = msg![env; this bounds];
    let scroll: id = msg_class![env; UIScrollView alloc];
    let scroll: id = msg![env; scroll initWithFrame:bounds];
    let width = bounds.size.width.max(1.0);
    let lines: usize = text
        .lines()
        .map(|line| ((line.chars().count() as f32 * 8.0 / width).ceil() as usize).max(1))
        .sum();
    let height = (lines.max(1) as f32 * 22.0).max(bounds.size.height);
    let label: id = msg_class![env; UILabel alloc];
    let label: id = msg![env; label initWithFrame:(CGRect { origin: Default::default(), size: CGSize { width, height } })];
    let text = from_rust_string(env, text);
    () = msg![env; label setText:text];
    release(env, text);
    () = msg![env; label setNumberOfLines:0i32];
    () = msg![env; label setOpaque:false];
    if white_text {
        let color: id = msg_class![env; UIColor whiteColor];
        () = msg![env; label setTextColor:color];
    }
    let font: id = msg_class![env; UIFont systemFontOfSize:14.0f32];
    () = msg![env; label setFont:font];
    () = msg![env; scroll addSubview:label];
    release(env, label);
    () = msg![env; scroll setContentSize:(CGSize { width, height })];
    () = msg![env; this addSubview:scroll];
    env.objc.borrow_mut::<WebViewHostObject>(this).scroll = scroll;
    let sel = env
        .objc
        .register_host_selector("webViewDidFinishLoad:".into(), &mut env.mem);
    if delegate != nil && msg![env; delegate respondsToSelector:sel] {
        () = msg![env; delegate webViewDidFinishLoad:this];
    }
}
- (())loadRequest:(id)request {
    let url: id = msg![env; request URL];
    let description: id = msg![env; url description];
    log!(
        "UIWebView external requests are unsupported: {}",
        to_rust_string(env, description)
    );
}
- (())dealloc {
    let host = env.objc.borrow::<WebViewHostObject>(this);
    let refs = [host.html, host.base_url, host.scroll];
    for value in refs {
        release(env, value);
    }
    msg_super![env; this dealloc]
}
@end
};

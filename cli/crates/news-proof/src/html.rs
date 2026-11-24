//! Just enough HTML reading for the checks: tags with attributes, the text of elements, visible text. The
//! renderer writes well-formed HTML, so this is a scanner, not a parser; it never builds a tree.
//! shortcut: no HTML5 error recovery, upgrade only if a check starts reading hand-written HTML.

#[derive(Debug, Clone, PartialEq)]
pub struct Tag {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    /// Byte offset just after the `>`.
    pub end: usize,
}

impl Tag {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
    pub fn has_class(&self, class: &str) -> bool {
        self.attr("class")
            .is_some_and(|c| c.split_whitespace().any(|x| x == class))
    }
}

/// Every opening tag, in order. Comments, closing tags and the bodies of `script` and `style` are skipped.
pub fn tags(html: &str) -> Vec<Tag> {
    let b = html.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(off) = html[i..].find('<') {
        let start = i + off;
        let rest = &html[start..];
        if rest.starts_with("<!--") {
            i = rest.find("-->").map_or(b.len(), |e| start + e + 3);
            continue;
        }
        let next = b.get(start + 1).copied().unwrap_or(0);
        if !next.is_ascii_alphabetic() {
            i = start + 1;
            continue;
        }
        let (tag, after) = read_tag(html, start);
        i = after;
        if matches!(tag.name.as_str(), "script" | "style") {
            let close = format!("</{}", tag.name);
            i = html[i..]
                .to_ascii_lowercase()
                .find(&close)
                .map_or(b.len(), |e| i + e);
        }
        out.push(tag);
    }
    out
}

fn read_tag(html: &str, start: usize) -> (Tag, usize) {
    let b = html.as_bytes();
    let mut i = start + 1;
    while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' && b[i] != b'/' {
        i += 1;
    }
    let name = html[start + 1..i].to_ascii_lowercase();
    let mut attrs = Vec::new();
    loop {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b'/') {
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        if b[i] == b'>' {
            i += 1;
            break;
        }
        let ks = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && !matches!(b[i], b'=' | b'>' | b'/') {
            i += 1;
        }
        let key = html[ks..i].to_ascii_lowercase();
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut val = String::new();
        if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                let q = b[i];
                let vs = i + 1;
                i = vs;
                while i < b.len() && b[i] != q {
                    i += 1;
                }
                val = decode(&html[vs..i.min(b.len())]);
                i = (i + 1).min(b.len());
            } else {
                let vs = i;
                while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' {
                    i += 1;
                }
                val = decode(&html[vs..i]);
            }
        }
        if !key.is_empty() {
            attrs.push((key, val));
        }
    }
    (
        Tag {
            name,
            attrs,
            end: i,
        },
        i,
    )
}

/// The decoded text between a tag's end and the next `</name>`, with nested tags removed.
pub fn inner_text(html: &str, tag: &Tag) -> String {
    let close = format!("</{}", tag.name);
    let rest = &html[tag.end..];
    let stop = rest.to_ascii_lowercase().find(&close).unwrap_or(rest.len());
    strip_tags(&rest[..stop])
}

/// The visible text of the whole document: tags, scripts and styles removed, entities decoded, whitespace
/// collapsed to single spaces.
pub fn visible_text(html: &str) -> String {
    let mut s = String::with_capacity(html.len());
    let mut i = 0;
    for t in tags(html) {
        // `t.end` is after the tag; its start is the last '<' before it.
        let start = html[..t.end].rfind('<').unwrap_or(0);
        if start >= i {
            s.push_str(&html[i..start]);
            s.push(' ');
        }
        i = t.end;
        if matches!(t.name.as_str(), "script" | "style") {
            let close = format!("</{}", t.name);
            i = html[i..]
                .to_ascii_lowercase()
                .find(&close)
                .map_or(html.len(), |e| i + e);
        }
    }
    if i < html.len() {
        s.push_str(&html[i..]);
    }
    strip_tags(&s)
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    collapse(&decode(&out))
}

pub fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Decodes the entities the renderer writes (`&amp; &lt; &gt; &quot; &#39;` and numeric ones).
pub fn decode(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(semi) = tail.find(';').filter(|&e| e <= 10) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let ent = &tail[1..semi];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            _ => ent.strip_prefix('#').and_then(|n| {
                let code = match n.strip_prefix(['x', 'X']) {
                    Some(h) => u32::from_str_radix(h, 16).ok(),
                    None => n.parse().ok(),
                };
                code.and_then(char::from_u32)
            }),
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &tail[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn reads_tags_attributes_and_skips_scripts_and_comments() {
        let html = r#"<!doctype html><!-- <b x="1"> --><meta name="robots" content="noindex"><script>if (a<b) {}</script><img SRC='/a.png' alt="A &amp; B" hidden><p class="a b">x</p>"#;
        let t = tags(html);
        let names: Vec<_> = t.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["meta", "script", "img", "p"]);
        assert_eq!(t[0].attr("name"), Some("robots"));
        assert_eq!(t[2].attr("src"), Some("/a.png"));
        assert_eq!(t[2].attr("alt"), Some("A & B"));
        assert_eq!(t[2].attr("hidden"), Some(""));
        assert!(t[3].has_class("b") && !t[3].has_class("c"));
    }

    #[test]
    fn text_is_decoded_and_collapsed() {
        let html = "<h1>Sales  &amp; <b>profit</b></h1><script>var x = 1;</script><p>&#8722;5.7%&nbsp;ok &bogus;</p>";
        assert_eq!(visible_text(html), "Sales & profit \u{2212}5.7% ok &bogus;");
        let t = &tags(html)[0];
        assert_eq!(inner_text(html, t), "Sales & profit");
    }
}

//! Reading the Copy: the string literals of `page.oui` and the head text, split into reference tokens
//! (`{dp_…}`, and whole-reference props such as `"dp_…"`) and the prose left over (Bureau §3.8, §3.9, C4).

use news_core::refs::is_ref;

/// One piece of page-writer text and where it sits.
#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    /// `page.oui:12` or `meta.json:title`.
    pub at: String,
    pub text: String,
    /// A media field copied from the brief's embed record (a video id, a date): not prose.
    pub media: bool,
}

impl Text {
    pub fn new(at: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            at: at.into(),
            text: text.into(),
            media: false,
        }
    }
    /// Written by the page writer in words: not a whole reference, a URL or a media field.
    pub fn is_prose(&self) -> bool {
        !self.media && is_prose(&self.text)
    }
}

/// Marks the media fields of `VideoEmbed(title, publisher, url, youtubeId, publishedOn)`: everything after
/// the title and publisher. URLs and image sizes are already not prose (`is_prose`, numbers are not strings).
pub fn mark_media(src: &str, texts: &mut [Text]) {
    let starts = statements(src);
    let lines: Vec<&str> = src.lines().collect();
    let mut seen: Option<(usize, usize)> = None; // (statement line, literals so far)
    for t in texts.iter_mut() {
        let Some(line) =
            t.at.strip_prefix("page.oui:")
                .and_then(|n| n.parse::<usize>().ok())
        else {
            continue;
        };
        let Some(&(start, _)) = starts.iter().rev().find(|(l, _)| *l <= line) else {
            continue;
        };
        let n = match seen {
            Some((s, n)) if s == start => n,
            _ => 0,
        };
        seen = Some((start, n + 1));
        let call = lines[start - 1]
            .split_once('=')
            .map_or("", |(_, r)| r.trim_start());
        if call.starts_with("VideoEmbed(") && n >= 2 {
            t.media = true;
        }
    }
}

/// Every double-quoted string literal in `src`, with its line. `Err` for an unterminated string.
pub fn string_literals(src: &str) -> Result<Vec<Text>, String> {
    let mut out = Vec::new();
    let mut chars = src.char_indices().peekable();
    let mut line = 1;
    while let Some((_, c)) = chars.next() {
        match c {
            '\n' => line += 1,
            '"' => {
                let start_line = line;
                let mut s = String::new();
                let mut closed = false;
                while let Some((_, c)) = chars.next() {
                    match c {
                        '\\' => match chars.next() {
                            Some((_, 'n')) => s.push('\n'),
                            Some((_, 't')) => s.push('\t'),
                            Some((_, e)) => s.push(e),
                            None => break,
                        },
                        '"' => {
                            closed = true;
                            break;
                        }
                        '\n' => {
                            line += 1;
                            s.push('\n');
                        }
                        _ => s.push(c),
                    }
                }
                if !closed {
                    return Err(format!("line {start_line}: string is not closed"));
                }
                out.push(Text::new(format!("page.oui:{start_line}"), s));
            }
            _ => {}
        }
    }
    Ok(out)
}

/// The statement names `page.oui` defines (`name = …` at the start of a line).
pub fn statements(src: &str) -> Vec<(usize, String)> {
    src.lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let (name, rest) = l.split_once('=')?;
            let name = name.trim();
            let ok = !name.is_empty()
                && name.starts_with(|c: char| c.is_ascii_lowercase())
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !rest.starts_with('=');
            ok.then(|| (i + 1, name.to_owned()))
        })
        .collect()
}

/// Structure problems the OpenUI parser would stop on (C1): unbalanced brackets, no `root`, an unquoted
/// reference, a statement that is used but never defined. The real parse is the renderer's; this catches
/// a bundle whose `page.oui` cannot have rendered.
pub fn structure_problems(src: &str) -> Vec<String> {
    let mut problems = Vec::new();
    if let Err(e) = string_literals(src) {
        problems.push(e);
        return problems;
    }
    let defined: Vec<String> = statements(src).into_iter().map(|(_, n)| n).collect();
    if !defined.iter().any(|n| n == "root") {
        problems.push("no `root = …` statement".into());
    }
    // Drop the string contents, then check brackets and bare identifiers on what is left.
    let mut code = String::with_capacity(src.len());
    let mut in_str = false;
    let mut esc = false;
    for c in src.chars() {
        if in_str {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
                code.push('"');
            } else if c == '\n' {
                code.push('\n');
            }
        } else {
            if c == '"' {
                in_str = true;
            }
            code.push(c);
        }
    }
    let mut stack = Vec::new();
    for (n, l) in code.lines().enumerate() {
        for c in l.chars() {
            match c {
                '(' | '[' | '{' => stack.push((c, n + 1)),
                ')' | ']' | '}' => match stack.pop() {
                    Some((o, _)) if matches!((o, c), ('(', ')') | ('[', ']') | ('{', '}')) => {}
                    _ => problems.push(format!("line {}: unmatched `{c}`", n + 1)),
                },
                _ => {}
            }
        }
    }
    if let Some((o, n)) = stack.pop() {
        problems.push(format!("line {n}: `{o}` is never closed"));
    }
    for (n, l) in code.lines().enumerate() {
        let lead_def = l.split_once('=').map(|(a, _)| a.trim().to_owned());
        let b: Vec<char> = l.chars().collect();
        let mut i = 0;
        while i < b.len() {
            if b[i].is_ascii_alphabetic() || b[i] == '_' {
                let s = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == '_') {
                    i += 1;
                }
                let word: String = b[s..i].iter().collect();
                let next = b[i..].iter().find(|c| !c.is_whitespace());
                let prev_dot = s > 0 && b[s - 1] == '.';
                let is_def = lead_def.as_deref() == Some(word.as_str())
                    && b[..s].iter().all(|c| c.is_whitespace());
                let skip = matches!(next, Some('(' | ':'))
                    || is_def
                    || prev_dot
                    || matches!(word.as_str(), "true" | "false" | "null")
                    || word.starts_with(|c: char| c.is_ascii_uppercase());
                if skip {
                    continue;
                }
                if is_ref(&word) {
                    problems.push(format!(
                        "line {}: reference `{word}` is not quoted (a bare identifier is a parse error)",
                        n + 1
                    ));
                } else if !defined.contains(&word) {
                    problems.push(format!("line {}: `{word}` is never defined", n + 1));
                }
            } else if b[i].is_ascii_digit() {
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == '.') {
                    i += 1;
                }
            } else {
                i += 1;
            }
        }
    }
    problems.dedup();
    problems
}

/// A string split into its reference tokens and the prose around them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Split {
    /// `{ref}` tokens, in order.
    pub tokens: Vec<String>,
    /// The prose with each token, and each Markdown link target `(…)`, replaced by a space.
    pub prose: String,
    /// Braces that do not hold a reference.
    pub stray_braces: Vec<String>,
}

pub fn split(text: &str) -> Split {
    let mut out = Split::default();
    let mut rest = text;
    while let Some(i) = rest.find('{') {
        out.prose.push_str(&rest[..i]);
        let tail = &rest[i + 1..];
        match tail.find('}') {
            Some(j) if is_ref(&tail[..j]) => {
                out.tokens.push(tail[..j].to_owned());
                out.prose.push(' ');
                rest = &tail[j + 1..];
            }
            Some(j) => {
                out.stray_braces.push(tail[..j].to_owned());
                out.prose.push_str(&rest[i..=i + 1 + j]);
                rest = &tail[j + 1..];
            }
            None => {
                out.stray_braces.push(tail.to_owned());
                out.prose.push_str(&rest[i..]);
                rest = "";
            }
        }
    }
    out.prose.push_str(rest);
    out.prose = strip_link_targets(&out.prose);
    out
}

/// `[words](glossary:slug)` and `[words](https://…)`: the target is not prose.
fn strip_link_targets(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("](") {
        out.push_str(&rest[..=i]);
        let tail = &rest[i + 2..];
        match tail.find(')') {
            Some(j) => {
                out.push(' ');
                rest = &tail[j + 1..];
            }
            None => {
                out.push('(');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

/// A prop that is only a URL or a whole reference is not prose.
pub fn is_prose(text: &str) -> bool {
    let t = text.trim();
    !(is_ref(t)
        || t.starts_with("https://")
        || t.starts_with("http://")
        || t.starts_with('/') && !t.contains(' '))
}

/// Every reference in the Copy: `{ref}` tokens inside text, and strings that are a whole reference.
pub fn refs_in(texts: &[Text]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for t in texts {
        let trimmed = t.text.trim();
        if is_ref(trimmed) {
            out.push((t.at.clone(), trimmed.to_owned()));
        } else {
            for r in split(&t.text).tokens {
                out.push((t.at.clone(), r));
            }
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn finds_strings_with_escapes_and_lines() {
        let src = "root = Page([a])\na = Text(\"Sales \\\"rose\\\" {dp_x}\", \"dp_y\")\n";
        let s = string_literals(src).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].at, "page.oui:2");
        assert_eq!(s[0].text, "Sales \"rose\" {dp_x}");
        assert!(string_literals("a = Text(\"open").is_err());
    }

    #[test]
    fn splits_tokens_from_prose_and_drops_link_targets() {
        let s = split(
            "Rose to {dp_a} in {pd_2026_10}: a [glossary term](glossary:cpi) {not a ref} and {x",
        );
        assert_eq!(s.tokens, ["dp_a", "pd_2026_10"]);
        assert!(s.prose.contains("glossary term"));
        assert!(!s.prose.contains("glossary:cpi"));
        assert_eq!(s.stray_braces.len(), 2);
    }

    #[test]
    fn whole_reference_props_and_urls_are_not_prose() {
        assert!(!is_prose("dp_aapl_net_sales_fy2024"));
        assert!(!is_prose("https://images.nasa.gov/a1.jpg"));
        assert!(!is_prose("/images/a1.jpg"));
        assert!(is_prose("Net sales"));
    }

    #[test]
    fn structure_problems_catch_what_the_parser_would() {
        let good = "root = Page([a, b])\na = Text(\"x\")\nb = Row({label: \"y\", value: \"dp_x\"}, null)\n";
        assert!(
            structure_problems(good).is_empty(),
            "{:?}",
            structure_problems(good)
        );
        assert!(structure_problems("a = Text(\"x\")\n")[0].contains("root"));
        assert!(
            structure_problems("root = Page([a)\na = Text(\"x\")\n")
                .iter()
                .any(|p| p.contains("unmatched"))
        );
        let bare = structure_problems("root = Page([a])\na = Figure(dp_x)\n");
        assert!(bare.iter().any(|p| p.contains("not quoted")), "{bare:?}");
        let undefined = structure_problems("root = Page([a, missing])\na = Text(\"x\")\n");
        assert!(
            undefined.iter().any(|p| p.contains("never defined")),
            "{undefined:?}"
        );
        assert!(
            structure_problems("root = Page([a])\na = Text(\"open\n")[0].contains("not closed")
        );
    }
}

//! robots.txt per RFC 9309: groups by user-agent (case-insensitive product token, falling back to
//! `*`), longest-match rules with `*` and `$`, `allow` winning ties.

/// What a host's robots.txt says, as the Runner caches it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Robots {
    /// 4xx on robots.txt (RFC 9309 §2.3.1.3): no restrictions.
    AllowAll,
    /// 5xx or unreachable (§2.3.1.4): assume complete disallow.
    DisallowAll,
    Rules(Vec<Group>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub agents: Vec<String>,
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub allow: bool,
    pub pattern: String,
}

/// Parse a robots.txt body. Unknown lines (Crawl-delay, Sitemap, junk) are ignored, as §2.2 asks.
pub fn parse(body: &str) -> Robots {
    let mut groups: Vec<Group> = Vec::new();
    let mut in_agents = false;
    for raw in body.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let (key, value) = (key.trim().to_ascii_lowercase(), value.trim());
        match key.as_str() {
            "user-agent" => {
                if !in_agents {
                    groups.push(Group {
                        agents: Vec::new(),
                        rules: Vec::new(),
                    });
                    in_agents = true;
                }
                if let Some(g) = groups.last_mut() {
                    g.agents.push(value.to_ascii_lowercase());
                }
            }
            "allow" | "disallow" => {
                in_agents = false;
                if let Some(g) = groups.last_mut() {
                    // An empty disallow allows everything: no rule.
                    if !value.is_empty() {
                        g.rules.push(Rule {
                            allow: key == "allow",
                            pattern: value.to_owned(),
                        });
                    }
                }
            }
            _ => {}
        }
    }
    Robots::Rules(groups)
}

impl Robots {
    /// May `agent` (a product token such as `News`) fetch `path` (path plus query)?
    pub fn allowed(&self, agent: &str, path: &str) -> bool {
        let groups = match self {
            Robots::AllowAll => return true,
            Robots::DisallowAll => return false,
            Robots::Rules(g) => g,
        };
        let agent = agent.to_ascii_lowercase();
        let mine: Vec<&Group> = groups
            .iter()
            .filter(|g| g.agents.contains(&agent))
            .collect();
        let chosen = if mine.is_empty() {
            groups
                .iter()
                .filter(|g| g.agents.iter().any(|a| a == "*"))
                .collect()
        } else {
            mine
        };
        // Longest matching pattern wins; on a tie, allow wins (§2.2.2).
        let mut best: Option<(usize, bool)> = None;
        for r in chosen.iter().flat_map(|g| &g.rules) {
            if matches(&r.pattern, path) {
                let len = r.pattern.len();
                best = match best {
                    Some((l, a)) if l > len || (l == len && a) => Some((l, a)),
                    _ => Some((len, r.allow)),
                };
            }
        }
        best.is_none_or(|(_, allow)| allow)
    }
}

/// `*` matches any run of characters; a trailing `$` anchors the end.
fn matches(pattern: &str, path: &str) -> bool {
    let (pat, anchored) = match pattern.strip_suffix('$') {
        Some(p) => (p, true),
        None => (pattern, false),
    };
    let parts: Vec<&str> = pat.split('*').collect();
    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            if !path.starts_with(part) {
                return false;
            }
            pos = part.len();
            continue;
        }
        let last = i == parts.len() - 1;
        if last && anchored {
            return path.len() >= pos + part.len() && path.ends_with(part);
        }
        match path[pos..].find(part) {
            Some(off) => pos += off + part.len(),
            None => return false,
        }
    }
    !anchored || pos == path.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorded_robots_files() {
        let fred = parse(include_str!(
            "../../news-connectors/tests/fixtures/fred/api-robots.txt"
        ));
        assert!(fred.allowed("News", "/fred/series?series_id=CPIAUCSL"));
        let bls = parse(include_str!(
            "../../news-connectors/tests/fixtures/bls/api-robots.txt"
        ));
        assert!(!bls.allowed("News", "/publicAPI/v2/timeseries/data/CUUR0000SA0"));
    }

    #[test]
    fn rfc9309_matching() {
        let r = parse(
            "User-agent: *\nDisallow: /private\n\nUser-Agent: NEWS\nuser-agent: other\nDisallow: /\nAllow: /fred/\nDisallow: /fred/*.xml$\nAllow: /x\nDisallow: /x\n",
        );
        // Our group (case-insensitive) applies, not `*`.
        assert!(!r.allowed("News", "/a"));
        assert!(r.allowed("News", "/fred/series"));
        assert!(!r.allowed("News", "/fred/a/b.xml"));
        assert!(r.allowed("News", "/fred/a/b.xml?x=1"));
        assert!(r.allowed("News", "/x"), "allow wins a tie");
        assert!(!r.allowed("Other-bot", "/private"));
        assert!(r.allowed("SomeoneElse", "/public"));
        assert!(Robots::AllowAll.allowed("News", "/"));
        assert!(!Robots::DisallowAll.allowed("News", "/"));
        assert!(parse("").allowed("News", "/anything"));
    }
}

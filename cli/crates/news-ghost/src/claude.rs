//! One `claude -p` call: its argv (Bureau §3.9), its allow-listed environment, and how its JSON reply
//! is read (the usage-limit classifier of ADR 0008 §8.4).

use chrono::{DateTime, Utc};
use serde_json::Value;
use std::path::Path;
use uuid::Uuid;

/// `--max-turns` and `--max-budget-usd` of the §3.9 call shape. The budget is a client-side runaway
/// guard, not a bill.
pub const MAX_TURNS: &str = "2";
pub const MAX_BUDGET_USD: &str = "0.60";

/// What one call is given.
#[derive(Debug, Clone, Copy)]
pub struct CallSpec<'a> {
    pub model: &'a str,
    pub settings: &'a Path,
    pub prefix: Option<&'a Path>,
    /// The output schema's JSON text.
    pub schema: &'a str,
}

/// The §3.9 flag set, in order (after the executable). The lean-call spike (ADR 0014, a parallel
/// task) may change it; this function is the one place to do so.
pub fn argv(c: &CallSpec<'_>) -> Vec<String> {
    let mut a: Vec<String> = [
        "-p",
        "--model",
        c.model,
        "--tools",
        "",
        "--strict-mcp-config",
        "--mcp-config",
        r#"{"mcpServers":{}}"#,
        "--settings",
    ]
    .map(str::to_owned)
    .to_vec();
    a.push(c.settings.to_string_lossy().into_owned());
    // No user, project or local settings (and so no hooks, plugins or CLAUDE.md) beside `--settings`.
    // Bureau §3.9 names the flag but gives no value; the empty list is the lean-call spike's (ADR 0014)
    // to confirm.
    a.push("--setting-sources".into());
    a.push(String::new());
    a.push("--disable-slash-commands".into());
    a.push("--no-session-persistence".into());
    if let Some(p) = c.prefix {
        a.push("--append-system-prompt-file".into());
        a.push(p.to_string_lossy().into_owned());
    }
    for s in [
        "--json-schema",
        c.schema,
        "--max-turns",
        MAX_TURNS,
        "--max-budget-usd",
        MAX_BUDGET_USD,
        "--output-format",
        "json",
    ] {
        a.push(s.to_owned());
    }
    a
}

/// The only parent variables a call may see (compared case-insensitively): what Windows and Node need
/// to run and find Karthik's Claude login, and Claude Code's own telemetry switches (ADR 0008 §7.2:
/// Bureau's calls export `claude_code.token.usage`). Nothing that can hold a secret: no API keys, no
/// `DATABASE_URL`, no proxy, no `OTEL_EXPORTER_OTLP_HEADERS`.
pub const ENV_ALLOW: &[&str] = &[
    "ALLUSERSPROFILE",
    "APPDATA",
    "CLAUDE_CONFIG_DIR",
    "CLAUDE_CODE_ENABLE_TELEMETRY",
    "COMMONPROGRAMFILES",
    "COMPUTERNAME",
    "COMSPEC",
    "HOME",
    "HOMEDRIVE",
    "HOMEPATH",
    "LANG",
    "LOCALAPPDATA",
    "NUMBER_OF_PROCESSORS",
    "OS",
    "OTEL_EXPORTER_OTLP_ENDPOINT",
    "OTEL_EXPORTER_OTLP_PROTOCOL",
    "OTEL_LOGS_EXPORTER",
    "OTEL_METRICS_EXPORTER",
    "PATH",
    "PATHEXT",
    "PROCESSOR_ARCHITECTURE",
    "PROGRAMDATA",
    "PROGRAMFILES",
    "PROGRAMFILES(X86)",
    "PROGRAMW6432",
    "PUBLIC",
    "SYSTEMDRIVE",
    "SYSTEMROOT",
    "TEMP",
    "TMP",
    "TMPDIR",
    "USER",
    "USERDOMAIN",
    "USERNAME",
    "USERPROFILE",
    "WINDIR",
];

/// Variables Ghost sets itself on every call.
pub const ENV_SET: &[&str] = &[
    "CLAUDE_CODE_DISABLE_AUTO_MEMORY",
    "NEWS_SPAWNED_BY",
    "OTEL_RESOURCE_ATTRIBUTES",
];

/// The child's whole environment, built up from the allow-list (never by removing names from the
/// parent's): `ANTHROPIC_API_KEY` is not on it, so `-p` uses the subscription. `NEWS_SPAWNED_BY` is
/// a label for logs only; the Hotline's check is the Job Object, never this.
pub fn child_env(
    parent: &[(String, String)],
    run: Option<Uuid>,
    job: i64,
    step: &str,
) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = parent
        .iter()
        .filter(|(k, _)| ENV_ALLOW.contains(&k.to_ascii_uppercase().as_str()))
        .cloned()
        .collect();
    let run = run.map(|r| r.to_string()).unwrap_or_default();
    env.push(("CLAUDE_CODE_DISABLE_AUTO_MEMORY".into(), "1".into()));
    env.push(("NEWS_SPAWNED_BY".into(), format!("{run}:{job}")));
    env.push((
        "OTEL_RESOURCE_ATTRIBUTES".into(),
        format!("news.run_id={run},news.job_id={job},news.step={step}"),
    ));
    env
}

/// How a call ended (ADR 0008 §8.4).
#[derive(Debug, Clone, PartialEq)]
pub enum ClaudeOutcome {
    Ok,
    UsageLimit { resets_at: Option<DateTime<Utc>> },
    Failed,
}

/// Result subtypes Claude Code documents for `--output-format json`.
const KNOWN_SUBTYPES: &[&str] = &[
    "success",
    "error_max_turns",
    "error_during_execution",
    "error_max_budget_usd",
    "error_max_structured_output_retries",
];

/// The wordings that mean the plan's limit, not any "limit reached" (a tool, a rate or a size limit).
fn is_limit_text(lower: &str) -> bool {
    [
        "usage limit",
        "5-hour limit reached",
        "weekly limit reached",
    ]
    .iter()
    .any(|p| lower.contains(p))
}

/// Classify a finished call from its exit code and stdout.
///
/// The usage-limit reply is not settled (ADR 0008 §1: no limit was hit in the spike), so this reads it
/// conservatively: a reply whose `result` text says the usage limit was reached is a limit, with the
/// reset taken from a trailing `|<unix seconds>` when there is one (the form Claude Code has printed);
/// and, as the ADR's provisional rule, a non-zero exit with `is_error: true` and an unknown `subtype` is
/// a limit with no known reset (Dispatch then pauses `ai` for its default hour and one call probes).
/// The first real limit fixes the shape here, in this one function.
pub fn classify(exit_code: Option<i32>, stdout: &str) -> ClaudeOutcome {
    let Some(v) = last_json(stdout) else {
        return ClaudeOutcome::Failed;
    };
    let is_error = v["is_error"].as_bool().unwrap_or(false);
    let text = v["result"].as_str().unwrap_or_default();
    let lower = text.to_ascii_lowercase();
    if is_error && is_limit_text(&lower) {
        let resets_at = text
            .rsplit_once('|')
            .and_then(|(_, t)| t.trim().parse::<i64>().ok())
            .and_then(|s| DateTime::from_timestamp(s, 0));
        return ClaudeOutcome::UsageLimit { resets_at };
    }
    let subtype = v["subtype"].as_str().unwrap_or_default();
    if exit_code != Some(0) && is_error && !KNOWN_SUBTYPES.contains(&subtype) {
        return ClaudeOutcome::UsageLimit { resets_at: None };
    }
    if exit_code == Some(0) && !is_error && subtype == "success" {
        ClaudeOutcome::Ok
    } else {
        ClaudeOutcome::Failed
    }
}

/// The reply object: the last line of stdout that is a JSON object (the result is printed last).
pub fn last_json(stdout: &str) -> Option<Value> {
    if let Ok(v @ Value::Object(_)) = serde_json::from_str::<Value>(stdout.trim()) {
        return Some(v);
    }
    stdout
        .lines()
        .rev()
        .find_map(|l| match serde_json::from_str::<Value>(l.trim()) {
            Ok(v @ Value::Object(_)) => Some(v),
            _ => None,
        })
}

/// Token and timing figures from a reply's `usage` and top-level fields; missing ones are 0.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Usage {
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cache_read: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    pub turns: u32,
    pub cost_usd: f64,
}

impl Usage {
    pub fn from_reply(v: &Value) -> Self {
        let u = &v["usage"];
        let n = |x: &Value| x.as_u64().unwrap_or(0);
        let write_total = n(&u["cache_creation_input_tokens"]);
        let w5 = n(&u["cache_creation"]["ephemeral_5m_input_tokens"]);
        let w1h = n(&u["cache_creation"]["ephemeral_1h_input_tokens"]);
        Self {
            tokens_in: n(&u["input_tokens"]),
            tokens_out: n(&u["output_tokens"]),
            cache_read: n(&u["cache_read_input_tokens"]),
            // Without the split, count every cache write as 5-minute ones.
            cache_write_5m: if w5 + w1h == 0 { write_total } else { w5 },
            cache_write_1h: w1h,
            turns: u32::try_from(n(&v["num_turns"])).unwrap_or(u32::MAX),
            cost_usd: v["total_cost_usd"].as_f64().unwrap_or(0.0),
        }
    }

    /// Input-equivalent tokens, by Rota's one rule (cache reads are not counted).
    pub fn tokens(&self) -> u64 {
        news_rota::tokens(
            self.tokens_in,
            self.tokens_out,
            self.cache_write_5m + self.cache_write_1h,
            self.cache_read,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_argv_is_the_bureau_call_shape() {
        let a = argv(&CallSpec {
            model: "sonnet",
            settings: Path::new("s.json"),
            prefix: Some(Path::new("p.md")),
            schema: "{}",
        });
        assert_eq!(
            a.join(" "),
            r#"-p --model sonnet --tools  --strict-mcp-config --mcp-config {"mcpServers":{}} --settings s.json --setting-sources  --disable-slash-commands --no-session-persistence --append-system-prompt-file p.md --json-schema {} --max-turns 2 --max-budget-usd 0.60 --output-format json"#
        );
        assert_eq!(a[4], "", "--tools takes an empty value");
        let at = a.iter().position(|x| x == "--setting-sources").unwrap();
        assert_eq!(a[at + 1], "", "no setting source loads beside --settings");
        assert!(
            !a.iter().any(|x| x == "--bare"),
            "--bare never reads the subscription login"
        );
        let no_prefix = argv(&CallSpec {
            prefix: None,
            model: "haiku",
            settings: Path::new("s.json"),
            schema: "{}",
        });
        assert!(!no_prefix.iter().any(|x| x == "--append-system-prompt-file"));
    }

    #[test]
    fn the_child_env_is_built_from_the_allow_list() {
        let parent: Vec<(String, String)> = [
            ("Path", "C:\\bin"),
            ("USERPROFILE", "C:\\Users\\k"),
            ("ANTHROPIC_API_KEY", "SENTINEL-1"),
            ("AWS_SECRET_ACCESS_KEY", "SENTINEL-2"),
            ("CLOUDFLARE_API_TOKEN", "SENTINEL-3"),
            (news_core::secrets::WRITE_API_KEY_VAR, "SENTINEL-4"),
            ("DATABASE_URL", "postgres://u:SENTINEL-5@h/db"),
            ("OTEL_EXPORTER_OTLP_HEADERS", "authorization=SENTINEL-6"),
            ("CLAUDE_CODE_DISABLE_AUTO_MEMORY", "0"),
        ]
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .to_vec();
        let run = Uuid::nil();
        let env = child_env(&parent, Some(run), 42, "A1");
        assert!(!env.iter().any(|(_, v)| v.contains("SENTINEL")), "{env:?}");
        assert!(env.iter().any(|(k, v)| k == "Path" && v == "C:\\bin"));
        let get = |k: &str| {
            env.iter()
                .filter(|(n, _)| n == k)
                .map(|(_, v)| v.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(get("CLAUDE_CODE_DISABLE_AUTO_MEMORY"), ["1"]);
        assert_eq!(get("NEWS_SPAWNED_BY"), [format!("{run}:42")]);
        assert_eq!(
            get("OTEL_RESOURCE_ATTRIBUTES"),
            [format!("news.run_id={run},news.job_id=42,news.step=A1")]
        );
        for (k, _) in &env {
            let k = k.to_ascii_uppercase();
            assert!(
                ENV_ALLOW.contains(&k.as_str()) || ENV_SET.contains(&k.as_str()),
                "{k}"
            );
        }
    }

    #[test]
    fn replies_are_classified_conservatively() {
        let ok = json!({"type": "result", "subtype": "success", "is_error": false}).to_string();
        assert_eq!(classify(Some(0), &ok), ClaudeOutcome::Ok);
        // A reply after other lines of output still reads.
        assert_eq!(
            classify(Some(0), &format!("warming up\n{ok}\n")),
            ClaudeOutcome::Ok
        );
        let limit = json!({"type": "result", "subtype": "success", "is_error": true,
                           "result": "Claude AI usage limit reached|1763906400"})
        .to_string();
        assert_eq!(
            classify(Some(1), &limit),
            ClaudeOutcome::UsageLimit {
                resets_at: DateTime::from_timestamp(1_763_906_400, 0)
            }
        );
        let worded = json!({"is_error": true, "result": "5-hour limit reached ∙ resets 3pm"});
        assert_eq!(
            classify(Some(1), &worded.to_string()),
            ClaudeOutcome::UsageLimit { resets_at: None }
        );
        // A bare "limit reached" is not the plan's limit: with a known subtype it is a plain failure.
        let bare = json!({"is_error": true, "subtype": "error_during_execution",
                          "result": "context limit reached"});
        assert_eq!(classify(Some(1), &bare.to_string()), ClaudeOutcome::Failed);
        // The ADR's provisional rule: an unknown subtype on an error exit.
        let unknown = json!({"is_error": true, "subtype": "error_rate_limit"}).to_string();
        assert_eq!(
            classify(Some(1), &unknown),
            ClaudeOutcome::UsageLimit { resets_at: None }
        );
        for (code, out) in [
            (
                Some(1),
                json!({"is_error": true, "subtype": "error_max_turns"}).to_string(),
            ),
            (Some(1), "not json".to_owned()),
            (None, String::new()),
            (
                Some(0),
                json!({"is_error": false, "subtype": "error_max_structured_output_retries"})
                    .to_string(),
            ),
        ] {
            assert_eq!(classify(code, &out), ClaudeOutcome::Failed, "{out}");
        }
    }

    #[test]
    fn usage_counts_input_equivalent_tokens_without_cache_reads() {
        let v = json!({"num_turns": 2, "total_cost_usd": 0.12, "usage": {
            "input_tokens": 100, "output_tokens": 50, "cache_read_input_tokens": 9000,
            "cache_creation_input_tokens": 300,
            "cache_creation": {"ephemeral_5m_input_tokens": 0, "ephemeral_1h_input_tokens": 300}}});
        let u = Usage::from_reply(&v);
        assert_eq!((u.cache_write_5m, u.cache_write_1h, u.turns), (0, 300, 2));
        assert_eq!(u.tokens(), 450);
        let flat = Usage::from_reply(&json!({"usage": {"cache_creation_input_tokens": 7}}));
        assert_eq!(flat.cache_write_5m, 7);
    }
}

//! `news schema <command>` and `news ai schema <step>`: the contracts in `contracts/cli/`, embedded at
//! build time (see build.rs), so the binary serves exactly the files in the repository.

use super::Reply;
use news_core::{NewsError, NewsResult};
use serde_json::{Value, json};

include!(concat!(env!("OUT_DIR"), "/contracts.rs"));

/// AI steps and the contract file that bounds their output (Bureau §7.1).
pub const AI_STEPS: &[(&str, &str)] = &[
    ("A1", "ai/page-draft.v1.schema.json"),
    ("A2", "ai/page-draft.v1.schema.json"),
    ("A3", "ai/a3.classify.v1.schema.json"),
    ("A4", "ai/a4.storymatch.v1.schema.json"),
    ("A5", "ai/a5.condense.v1.schema.json"),
    ("A6", "ai/a6.entity-pick.v1.schema.json"),
    ("A7", "ai/a7.spotcheck.v1.schema.json"),
    ("A9", "ai/a9.triage.v1.schema.json"),
    ("A12", "ai/a12.judge.v1.schema.json"),
    ("brief", "ai/brief.v1.schema.json"),
];

pub fn contract(path: &str) -> Option<&'static str> {
    CONTRACTS.iter().find(|(p, _)| *p == path).map(|(_, s)| *s)
}

fn parse(path: &str, text: &str) -> NewsResult<Value> {
    serde_json::from_str(text)
        .map_err(|e| NewsError::internal(format!("{path} is not valid JSON: {e}")))
}

/// Dotted command names that have a contract, sorted.
pub fn commands() -> Vec<String> {
    let mut v: Vec<String> = CONTRACTS
        .iter()
        .filter_map(|(p, _)| p.strip_prefix("commands/")?.strip_suffix(".schema.json"))
        .map(str::to_owned)
        .collect();
    v.sort();
    v
}

pub fn command(words: &[String]) -> NewsResult<Reply> {
    if words.is_empty() {
        let list = commands();
        return Ok(Reply::new(json!({
            "count": list.len(),
            "items": list.iter().map(|c| c.replace('.', " ")).collect::<Vec<_>>(),
            "envelope": parse("envelope.schema.json", contract("envelope.schema.json").unwrap_or("{}"))?,
        }))
        .default_limit(news_core::shape::MAX_LIMIT));
    }
    let dotted = words.join(".");
    let path = format!("commands/{dotted}.schema.json");
    let text = contract(&path).ok_or_else(|| {
        NewsError::not_found(format!("no schema for `news {}`", words.join(" ")))
            .with_hint("news schema")
    })?;
    Ok(Reply::new(
        json!({ "command": words.join(" "), "schema": parse(&path, text)? }),
    ))
}

pub fn ai(step: &str) -> NewsResult<Reply> {
    let (name, path) = AI_STEPS
        .iter()
        .find(|(s, _)| s.eq_ignore_ascii_case(step))
        .ok_or_else(|| {
            let names: Vec<&str> = AI_STEPS.iter().map(|(s, _)| *s).collect();
            NewsError::not_found(format!("no AI step {step:?}"))
                .with_hint(format!("one of {}", names.join(", ")))
        })?;
    let text = contract(path)
        .ok_or_else(|| NewsError::internal(format!("{path} missing from the build")))?;
    Ok(Reply::new(
        json!({ "step": name, "schema": parse(path, text)? }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ai_step_has_its_contract_embedded() {
        for (step, path) in AI_STEPS {
            assert!(contract(path).is_some(), "{step}: {path}");
        }
    }

    #[test]
    fn the_envelope_is_embedded() {
        assert!(contract("envelope.schema.json").is_some());
    }
}

//! What Board turns forms into: arguments for `run.start`, a `standing.set` from edited settings, a gate
//! answer from what was shown. Pure, so it is tested without a window. Nothing here converts a
//! number, unit, currency or period; values go to the service as typed, checked against the
//! declaration the service published (`run.presets`, `standing.get`).

use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// A form value meaning "reset to the default" (sent as `null`).
pub const RESET: &str = "\u{0}reset";

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or_default()
}

/// One argument from its text, as the declaration's type says. `Ok(None)` means "leave it out".
pub fn arg_value(decl: &Value, text: &str) -> Result<Option<Value>, String> {
    let name = s(decl, "name");
    let text = text.trim();
    if text.is_empty() {
        return if decl["required"] == true && decl["default"].is_null() {
            Err(format!("{name} is required."))
        } else {
            Ok(None)
        };
    }
    let v = match s(decl, "type") {
        "integer" => {
            let n: i64 = text
                .parse()
                .map_err(|_| format!("{name} must be a whole number."))?;
            if let Some(min) = decl["min"].as_i64()
                && n < min
            {
                return Err(format!("{name} must be at least {min}."));
            }
            if let Some(max) = decl["max"].as_i64()
                && n > max
            {
                return Err(format!("{name} must be at most {max}."));
            }
            json!(n)
        }
        "boolean" => match text {
            "true" => json!(true),
            "false" => json!(false),
            _ => return Err(format!("{name} must be true or false.")),
        },
        "date" => {
            chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
                .map_err(|_| format!("{name} must be a date as YYYY-MM-DD."))?;
            json!(text)
        }
        "enum" => {
            let ok = decl["choices"]
                .as_array()
                .is_some_and(|c| c.iter().any(|x| x == text));
            if !ok {
                return Err(format!("{name} must be one of its listed choices."));
            }
            json!(text)
        }
        "list" => json!(
            text.split(',')
                .map(str::trim)
                .filter(|x| !x.is_empty())
                .collect::<Vec<_>>()
        ),
        _ => json!(text),
    };
    Ok(Some(v))
}

/// The `args` object of `run.start` from a preset declaration and the form's texts.
pub fn start_args(preset: &Value, form: &HashMap<String, String>) -> Result<Value, String> {
    let mut out = Map::new();
    for decl in preset["args"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let name = s(decl, "name");
        let text = form.get(name).map_or("", String::as_str);
        if let Some(v) = arg_value(decl, text)? {
            out.insert(name.to_owned(), v);
        }
    }
    Ok(Value::Object(out))
}

/// A setting's edited text as the value `standing.set` takes.
pub fn setting_value(item: &Value, text: &str) -> Result<Value, String> {
    let key = s(item, "key");
    if text == RESET {
        return Ok(Value::Null);
    }
    let text = text.trim();
    let nullable = text.is_empty();
    Ok(match s(item, "type") {
        "integer" => {
            let decl = json!({"name": key, "type": "integer", "required": true, "default": null,
                              "min": item["min"], "max": item["max"]});
            arg_value(&decl, text)?.ok_or_else(|| format!("{key} needs a whole number."))?
        }
        "tokens" => {
            if nullable {
                Value::Null
            } else {
                text.replace([',', '_'], "")
                    .parse::<i64>()
                    .map(|n| json!(n))
                    .map_err(|_| {
                        format!("{key} must be a whole number of tokens, or empty for no cap.")
                    })?
            }
        }
        "boolean" => match text {
            "true" => json!(true),
            "false" => json!(false),
            _ => return Err(format!("{key} must be on or off.")),
        },
        // A decimal goes as typed (0.05 stays 0.05); the service checks its range.
        "decimal" => text
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .map(|n| json!(n))
            .ok_or_else(|| format!("{key} must be a number."))?,
        "time_zone" => {
            if nullable {
                Value::Null
            } else {
                json!(text)
            }
        }
        "enum" => {
            if item["choices"]
                .as_array()
                .is_some_and(|c| c.iter().any(|x| x == text))
            {
                json!(text)
            } else {
                return Err(format!("{key} must be one of its listed choices."));
            }
        }
        "schedule" => {
            serde_json::from_str::<Value>(text).map_err(|e| format!("{key} must be JSON: {e}"))?
        }
        // time_of_day and text go as typed; the service validates the form.
        _ => json!(text),
    })
}

/// The `changes` of a `standing.set`: only keys whose edited value differs from the current one.
pub fn standing_changes(
    items: &Value,
    edits: &HashMap<String, String>,
) -> Result<Vec<Value>, String> {
    let mut out = Vec::new();
    let mut keys: Vec<&String> = edits.keys().collect();
    keys.sort();
    for key in keys {
        // A key that is not listed yet is a new schedule or a source switch.
        let known = items
            .as_array()
            .and_then(|a| a.iter().find(|i| i["key"] == key.as_str()))
            .cloned();
        let item = match known {
            Some(i) => i,
            None if key.starts_with("schedules.") => {
                json!({"key": key, "type": "schedule", "value": null})
            }
            None if key.starts_with("sources.") && key.ends_with(".enabled") => {
                json!({"key": key, "type": "boolean", "value": null})
            }
            None => continue,
        };
        let value = setting_value(&item, &edits[key])?;
        if value != item["value"] {
            out.push(json!({"key": key, "value": value}));
        }
    }
    Ok(out)
}

/// A gate answer's command and arguments, bound to the round of the gate Board showed (`gate` is a
/// `gateOpen`) and to the hash of what it showed, recomputed here from the gate's summary and preview
/// rather than echoed from the service. `changes` needs a note.
pub fn gate_answer(gate: &Value, decision: &str, note: &str) -> Result<(String, Value), String> {
    let note = note.trim();
    let cmd = match decision {
        "approve" => "gate.approve",
        "changes" => "gate.changes",
        "reject" => "gate.reject",
        other => return Err(format!("unknown decision {other:?}")),
    };
    if cmd == "gate.changes" && note.is_empty() {
        return Err("Say what to change before sending it back.".to_owned());
    }
    let shown = news_core::tier::gate_sha(&gate["summary"], gate.get("preview"));
    let mut args = json!({
        "run": gate["short_id"], "gate": gate["gate"], "round": gate["round"],
        "subject_sha": news_core::tier::hex(&shown),
    });
    if !note.is_empty() {
        args["note"] = json!(note);
    }
    Ok((cmd.to_owned(), args))
}

/// A form step the run waits at (Huddle's calibration), as Board shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct FormOpen {
    pub run: String,
    /// The step's handler, what `form.submit` names: `huddle.calibration`.
    pub form: String,
    pub round: i64,
    /// The hash of the form as displayed, recomputed here from the form Board holds, never the
    /// service's `form_sha` echoed back: a form that changed since Board loaded it is then refused.
    pub sha: String,
    pub text: String,
    pub questions: Vec<Value>,
}

/// The open form of a run (`run.show`'s data): a running `form` step that has shown its form.
pub fn open_form(detail: &Value) -> Option<FormOpen> {
    let step = detail["steps"].as_array()?.iter().find(|s| {
        s["kind"] == "form" && s["status"] == "running" && s["summary"]["form"].is_object()
    })?;
    let form = &step["summary"]["form"];
    Some(FormOpen {
        run: s(&detail["run"], "short_id").to_owned(),
        form: format!("{}.{}", s(&detail["run"], "preset"), s(step, "id")),
        round: step["summary"]["round"].as_i64()?,
        sha: news_core::tier::hex(&news_core::tier::form_sha(form)),
        text: s(form, "text").to_owned(),
        questions: form["questions"].as_array().cloned().unwrap_or_default(),
    })
}

/// Karthik's answer to one pairwise question: the story shown first (A), the other (B), or neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pick {
    #[default]
    Skip,
    A,
    B,
}

/// The pages Karthik reads each week (D15), and the most notes the form takes.
pub const READING_ROWS: usize = 5;

/// `form.submit`'s arguments from the form as shown and what Karthik filled in. A question left on
/// Skip sends no label; a reading note needs its page; blank rows are dropped.
pub fn form_submit(
    open: &FormOpen,
    picks: &[Pick],
    search_console: &str,
    reading: &[(String, String)],
) -> Result<Value, String> {
    let mut labels = Vec::new();
    for (q, pick) in open.questions.iter().zip(picks) {
        if *pick == Pick::Skip {
            continue;
        }
        let (Some(a), Some(b)) = (q["a"]["story_id"].as_i64(), q["b"]["story_id"].as_i64()) else {
            return Err("A question in the form has no story ids. Open the run again.".to_owned());
        };
        let winner = if *pick == Pick::A { "a" } else { "b" };
        labels.push(json!({"a": a, "b": b, "winner": winner}));
    }
    let mut answer = json!({"labels": labels});
    let status = search_console.trim();
    if status.chars().count() > 2000 {
        return Err("The Search Console status is over 2,000 characters.".to_owned());
    }
    if !status.is_empty() {
        answer["search_console"] = json!(status);
    }
    let mut pages = Vec::new();
    for (page, note) in reading {
        let (page, note) = (page.trim(), note.trim());
        if page.is_empty() && note.is_empty() {
            continue;
        }
        if page.is_empty() {
            return Err("Name the page for each reading note.".to_owned());
        }
        if page.chars().count() > 200 || note.chars().count() > 2000 {
            return Err(format!("The reading note for {page} is too long."));
        }
        let mut p = json!({"page": page});
        if !note.is_empty() {
            p["note"] = json!(note);
        }
        pages.push(p);
    }
    if pages.len() > READING_ROWS {
        return Err(format!("The form takes at most {READING_ROWS} pages."));
    }
    if !pages.is_empty() {
        answer["reading"] = json!(pages);
    }
    Ok(
        json!({"run": open.run, "form": open.form, "round": open.round,
              "form_sha": open.sha, "answer": answer}),
    )
}

/// The queue offer inside a `conflict` from `run.start`, if that is what the error is.
pub fn queue_offer(details: &Value) -> Option<&Value> {
    (details["reason"] == "queue_offer").then_some(details)
}

/// `3 of 12`, or `3` when the total is not known.
pub fn progress_text(p: &Value) -> Option<String> {
    let done = p["done"].as_i64()?;
    Some(match p["total"].as_i64() {
        Some(t) => format!("{done} of {t}"),
        None => done.to_string(),
    })
}

/// A UTC timestamp in the laptop's time zone, `12 Nov 16:02`.
pub fn local_time(ts: &str) -> String {
    ts.parse::<chrono::DateTime<chrono::Utc>>().map_or_else(
        |_| ts.to_owned(),
        |t| {
            t.with_timezone(&chrono::Local)
                .format("%d %b %H:%M")
                .to_string()
        },
    )
}

/// The first eight characters of a hash, for display next to the buttons that are bound to it.
pub fn short_sha(sha: &str) -> &str {
    sha.get(..8).unwrap_or(sha)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn decl(name: &str, ty: &str) -> Value {
        json!({"name": name, "type": ty, "required": false, "default": null, "doc": "",
               "choices": null, "min": null, "max": null})
    }

    #[test]
    fn arguments_follow_their_declared_types() {
        let mut n = decl("pages", "integer");
        n["min"] = json!(1);
        n["max"] = json!(500);
        assert_eq!(arg_value(&n, " 120 ").unwrap(), Some(json!(120)));
        assert!(arg_value(&n, "0").is_err());
        assert!(arg_value(&n, "501").is_err());
        assert!(arg_value(&n, "1.5").is_err());
        assert_eq!(arg_value(&n, "").unwrap(), None);
        n["required"] = json!(true);
        assert!(arg_value(&n, "").is_err());
        assert_eq!(
            arg_value(&decl("d", "date"), "2026-11-12").unwrap(),
            Some(json!("2026-11-12"))
        );
        assert!(arg_value(&decl("d", "date"), "12/11/2026").is_err());
        let mut e = decl("mode", "enum");
        e["choices"] = json!(["a", "b"]);
        assert!(arg_value(&e, "c").is_err());
        assert_eq!(
            arg_value(&decl("l", "list"), "x, y,,z").unwrap(),
            Some(json!(["x", "y", "z"]))
        );
    }

    #[test]
    fn start_args_collect_only_what_was_filled_in() {
        let preset = json!({"args": [decl("count", "integer"), decl("note", "text")]});
        let mut form = HashMap::new();
        form.insert("count".to_owned(), "7".to_owned());
        assert_eq!(start_args(&preset, &form).unwrap(), json!({"count": 7}));
        form.insert("count".to_owned(), "seven".to_owned());
        assert!(start_args(&preset, &form).is_err());
    }

    fn items() -> Value {
        json!([
            {"key": "chief.stall_minutes", "type": "integer", "value": 15, "min": 1, "max": 10080},
            {"key": "quiet_hours.enabled", "type": "boolean", "value": false},
            {"key": "quota.bureau_daily_tokens", "type": "tokens", "value": null},
            {"key": "rota.time_zone", "type": "time_zone", "value": null},
            {"key": "quiet_hours.mode", "type": "enum", "value": "dont_start", "choices": ["dont_start", "reduced"]},
            {"key": "schedules.edition", "type": "schedule",
             "value": {"preset": "edition", "args": {}, "cron": "0 11 * * *", "enabled": true}},
        ])
    }

    #[test]
    fn settings_edits_become_one_set_with_only_the_changed_keys() {
        let mut e = HashMap::new();
        e.insert("chief.stall_minutes".to_owned(), "15".to_owned()); // unchanged
        e.insert("quiet_hours.enabled".to_owned(), "true".to_owned());
        e.insert(
            "quota.bureau_daily_tokens".to_owned(),
            "1,500,000".to_owned(),
        );
        e.insert("quiet_hours.mode".to_owned(), RESET.to_owned());
        let c = standing_changes(&items(), &e).unwrap();
        assert_eq!(
            c,
            vec![
                json!({"key": "quiet_hours.enabled", "value": true}),
                json!({"key": "quiet_hours.mode", "value": null}),
                json!({"key": "quota.bureau_daily_tokens", "value": 1_500_000}),
            ]
        );
        // A new schedule and a source switch are not listed yet but are sent.
        let mut fresh = HashMap::new();
        fresh.insert(
            "schedules.late".to_owned(),
            r#"{"preset":"edition","args":{},"cron":"0 20 * * *","enabled":false}"#.to_owned(),
        );
        fresh.insert("sources.fred.enabled".to_owned(), "false".to_owned());
        let c = standing_changes(&items(), &fresh).unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c[1], json!({"key": "sources.fred.enabled", "value": false}));
        let mut bad = HashMap::new();
        bad.insert("chief.stall_minutes".to_owned(), "0".to_owned());
        assert!(standing_changes(&items(), &bad).is_err());
        bad.clear();
        bad.insert("quiet_hours.mode".to_owned(), "later".to_owned());
        assert!(standing_changes(&items(), &bad).is_err());
    }

    #[test]
    fn a_schedule_edit_is_json_checked_before_it_is_sent() {
        let mut e = HashMap::new();
        e.insert(
            "schedules.edition".to_owned(),
            r#"{"preset":"edition","args":{},"cron":"0 12 * * *","enabled":true}"#.to_owned(),
        );
        let c = standing_changes(&items(), &e).unwrap();
        assert_eq!(c[0]["value"]["cron"], "0 12 * * *");
        e.insert("schedules.edition".to_owned(), "{not json".to_owned());
        assert!(standing_changes(&items(), &e).is_err());
    }

    #[test]
    fn a_gate_answer_is_bound_to_the_hash_and_round_shown() {
        let summary = json!({"text": "Plan", "subject_items": ["a"]});
        let preview = json!({"table": [1, 2]});
        let shown = news_core::tier::hex(&news_core::tier::gate_sha(&summary, Some(&preview)));
        // The service's own hash is not echoed: Board sends the hash of what it displays.
        let mut gate = json!({"short_id": "r_51d2", "gate": "plan", "round": 2, "subject_sha": "9c1e",
                              "summary": summary, "preview": preview});
        let (cmd, args) = gate_answer(&gate, "approve", "").unwrap();
        assert_eq!(cmd, "gate.approve");
        assert_eq!(
            args,
            json!({"run": "r_51d2", "gate": "plan", "round": 2, "subject_sha": shown})
        );
        gate["preview"] = json!({"table": [1, 3]});
        assert_ne!(
            gate_answer(&gate, "approve", "").unwrap().1["subject_sha"],
            shown
        );
        assert!(gate_answer(&gate, "changes", "  ").is_err());
        let (cmd, args) = gate_answer(&gate, "changes", "shorter headline").unwrap();
        assert_eq!(
            (cmd.as_str(), args["note"].as_str()),
            ("gate.changes", Some("shorter headline"))
        );
        assert_eq!(gate_answer(&gate, "reject", "").unwrap().0, "gate.reject");
    }

    fn form_detail() -> Value {
        let form = json!({"text": "2 pairwise questions", "questions": [
            {"a": {"story_id": 4, "title": "Fed holds", "total": 71.2},
             "b": {"story_id": 9, "title": "Apple results", "total": 64.0}},
            {"a": {"story_id": 12, "title": "RBI cuts", "total": 58.1},
             "b": {"story_id": 3, "title": "US CPI", "total": 80.5}}]});
        json!({"run": {"short_id": "r_hud1", "preset": "huddle"},
               "steps": [
                 {"seq": 1, "id": "debrief", "kind": "code", "status": "done", "summary": {"text": "ok"}},
                 {"seq": 2, "id": "calibration", "kind": "form", "status": "running",
                  "summary": {"text": "2 pairwise questions", "round": 1, "form_sha": "ignored", "form": form}}]})
    }

    #[test]
    fn the_open_form_is_the_running_form_step_with_its_hash_recomputed() {
        let d = form_detail();
        let open = open_form(&d).unwrap();
        assert_eq!(
            (open.run.as_str(), open.form.as_str(), open.round),
            ("r_hud1", "huddle.calibration", 1)
        );
        assert_eq!(open.questions.len(), 2);
        let shown = news_core::tier::hex(&news_core::tier::form_sha(
            &d["steps"][1]["summary"]["form"],
        ));
        assert_eq!(
            open.sha, shown,
            "the hash of what is displayed, not the service's field"
        );
        // A form with a different question hashes differently.
        let mut other = form_detail();
        other["steps"][1]["summary"]["form"]["questions"][0]["a"]["title"] = json!("Fed cuts");
        assert_ne!(open_form(&other).unwrap().sha, open.sha);
        // Not a form that is waiting: finished, or not a form at all.
        let mut done = form_detail();
        done["steps"][1]["status"] = json!("done");
        assert!(open_form(&done).is_none());
        assert!(open_form(&json!({"run": {}, "steps": []})).is_none());
    }

    #[test]
    fn the_answer_has_a_label_per_picked_question_and_drops_blank_rows() {
        let open = open_form(&form_detail()).unwrap();
        let reading = vec![
            ("2026/11/us-cpi".to_owned(), " Reads well ".to_owned()),
            (String::new(), String::new()),
            ("2026/11/fed".to_owned(), String::new()),
        ];
        let args = form_submit(
            &open,
            &[Pick::B, Pick::Skip],
            " No coverage problems ",
            &reading,
        )
        .unwrap();
        assert_eq!(
            args,
            json!({"run": "r_hud1", "form": "huddle.calibration", "round": 1, "form_sha": open.sha,
                   "answer": {"labels": [{"a": 4, "b": 9, "winner": "b"}],
                              "search_console": "No coverage problems",
                              "reading": [{"page": "2026/11/us-cpi", "note": "Reads well"},
                                          {"page": "2026/11/fed"}]}})
        );
        // Skipping every question and filling nothing in is a valid answer: an empty label list.
        let args = form_submit(&open, &[Pick::Skip, Pick::Skip], "", &[]).unwrap();
        assert_eq!(args["answer"], json!({"labels": []}));
        // A note without its page, too many pages and a too-long status are refused here.
        assert!(form_submit(&open, &[], "", &[(String::new(), "note".into())]).is_err());
        let many: Vec<(String, String)> =
            (0..6).map(|i| (format!("p{i}"), String::new())).collect();
        assert!(form_submit(&open, &[], "", &many).is_err());
        assert!(form_submit(&open, &[], &"x".repeat(2001), &[]).is_err());
    }

    #[test]
    fn a_question_without_story_ids_is_not_answered() {
        let mut d = form_detail();
        d["steps"][1]["summary"]["form"]["questions"][0]["a"] = json!({"title": "no id"});
        let open = open_form(&d).unwrap();
        assert!(form_submit(&open, &[Pick::A], "", &[]).is_err());
        assert!(form_submit(&open, &[Pick::Skip], "", &[]).is_ok());
    }

    #[test]
    fn small_formatters() {
        assert_eq!(
            progress_text(&json!({"done": 3, "total": 12})).as_deref(),
            Some("3 of 12")
        );
        assert_eq!(
            progress_text(&json!({"done": 3, "total": null})).as_deref(),
            Some("3")
        );
        assert_eq!(short_sha("9c1e5d7a22bb"), "9c1e5d7a");
        assert_eq!(short_sha("9c"), "9c");
        assert!(queue_offer(&json!({"reason": "queue_offer"})).is_some());
        assert!(queue_offer(&json!({"reason": "preset_live"})).is_none());
    }
}

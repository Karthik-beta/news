//! Toasts (ADR 0007, "Notifications"). A Windows service runs in session 0 and cannot show them, so
//! Board raises them from the service's event stream: a gate waiting, a run failed, a usage limit, a
//! source red, Huddle questions ready. Clicking one opens Board on that item (`--open <kind:id>`).
//! The text is the service's own code-rendered `text`; nothing here writes a number.

use crate::args::OpenTarget;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub title: String,
    pub body: String,
    pub open: OpenTarget,
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or_default()
}

fn target(kind: &str, id: String) -> OpenTarget {
    OpenTarget {
        kind: kind.to_owned(),
        id,
    }
}

/// The toast an event raises, if any.
pub fn from_event(topic: &str, data: &Value) -> Option<Toast> {
    match topic {
        "gate" if s(data, "state") == "open" => Some(Toast {
            title: "Approval waiting".to_owned(),
            body: s(data, "text").to_owned(),
            open: target(
                "gate",
                format!("{}.{}", s(data, "short_id"), s(data, "gate")),
            ),
        }),
        "run" if s(data, "status") == "failed" => Some(Toast {
            title: "Run failed".to_owned(),
            body: s(data, "text").to_owned(),
            open: target("run", s(data, "short_id").to_owned()),
        }),
        "usage"
            if matches!(
                s(data, "kind"),
                "limit_reached" | "reserve_reached" | "cap_reached"
            ) =>
        {
            Some(Toast {
                title: "Usage limit".to_owned(),
                body: s(data, "text").to_owned(),
                open: target("usage", String::new()),
            })
        }
        "source" if s(data, "health") == "red" => Some(Toast {
            title: "Source is red".to_owned(),
            body: s(data, "text").to_owned(),
            open: target("source", s(data, "source_id").to_owned()),
        }),
        "huddle" => Some(Toast {
            title: "Huddle questions ready".to_owned(),
            body: s(data, "text").to_owned(),
            open: target("huddle", s(data, "short_id").to_owned()),
        }),
        _ => None,
    }
}

/// Show a native toast; `on_click` gets the item's `kind:id` when Karthik clicks it while Board is
/// running. Fire and forget: a toast that cannot be shown is not an error worth a dialog.
#[cfg(windows)]
pub fn show(t: &Toast, on_click: impl Fn(String) + Send + 'static) {
    use tauri_winrt_notification::Toast as Native;
    let open = t.open.arg();
    let (title, body) = (t.title.clone(), t.body.clone());
    std::thread::spawn(move || {
        let _ = Native::new(Native::POWERSHELL_APP_ID)
            .title(&title)
            .text1(&body)
            .on_activated(move |_| {
                on_click(open.clone());
                Ok(())
            })
            .show();
    });
}

#[cfg(not(windows))]
pub fn show(_t: &Toast, _on_click: impl Fn(String) + Send + 'static) {}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_gate_opening_raises_a_toast_that_opens_that_gate() {
        let t = from_event(
            "gate",
            &json!({"state": "open", "short_id": "r_51d2", "gate": "plan",
                    "text": "Edition r_51d2 is waiting for your approval of the plan."}),
        )
        .unwrap();
        assert_eq!(t.title, "Approval waiting");
        assert_eq!(t.open.arg(), "gate:r_51d2.plan");
        assert_eq!(t.open.run(), Some("r_51d2"));
        assert!(t.body.contains("waiting for your approval"));
        // The answer to a gate raises nothing.
        assert!(from_event("gate", &json!({"state": "answered"})).is_none());
    }

    #[test]
    fn failures_limits_red_sources_and_huddles_raise_toasts_and_the_rest_do_not() {
        let run = from_event(
            "run",
            &json!({"status": "failed", "short_id": "r_9", "text": "x"}),
        );
        assert_eq!(run.unwrap().open.arg(), "run:r_9");
        assert!(from_event("run", &json!({"status": "running"})).is_none());
        assert!(from_event("usage", &json!({"kind": "limit_reached", "text": "x"})).is_some());
        assert!(from_event("usage", &json!({"kind": "limit_reset", "text": "x"})).is_none());
        let src = from_event(
            "source",
            &json!({"health": "red", "source_id": "fred", "text": "x"}),
        );
        assert_eq!(src.unwrap().open.arg(), "source:fred");
        assert!(from_event("source", &json!({"health": "amber"})).is_none());
        assert!(from_event("huddle", &json!({"short_id": "r_h", "text": "x"})).is_some());
        assert!(from_event("service", &json!({})).is_none());
    }
}

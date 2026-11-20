//! Board's state and what its controls do. Every H-tier action is a method that a click calls; each
//! sends one request over the Hotline and shows what the service says back (its own code-rendered
//! `text`). The window (`ui.rs`) only draws this state and calls these methods.

use crate::args::{OpenTarget, Screen};
use crate::link::{ErrorInfo, Link, LinkState, Msg};
use crate::logic;
use crate::secrets::{self, Entry, Store};
use crate::theme::Preference;
use crate::toast::{self, Toast};
use news_core::secrets::SECRETS;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// `version+commit` of this binary, as the service reports its own in `service.ping`.
pub fn own_build() -> String {
    format!(
        "{}+{}",
        news_core::VERSION,
        option_env!("NEWS_BUILD_COMMIT").unwrap_or("dev")
    )
}

#[derive(Debug, Clone, PartialEq)]
pub enum Conn {
    Connecting,
    /// The reason, and what the Service Control Manager says (stopped, last error), when known.
    Offline {
        reason: String,
        scm: Option<String>,
    },
    Online {
        /// False after an update: Board must be restarted before it sends another H-tier command.
        build_ok: bool,
        ping: Value,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Feedback {
    pub ok: bool,
    pub text: String,
}

/// A destructive click waiting for its second click.
#[derive(Debug, Clone, PartialEq)]
pub struct Confirm {
    pub label: String,
    pub cmd: String,
    pub args: Value,
}

/// The heavy slot is busy: Queue it, or pause the running one and start this.
#[derive(Debug, Clone, PartialEq)]
pub struct Offer {
    pub preset: String,
    pub args: Value,
    pub blocking: String,
    pub text: String,
}

#[derive(Default)]
pub struct Forms {
    pub preset: String,
    pub args: HashMap<String, String>,
    /// A note per open gate, keyed `<run>.<gate>`.
    pub gate_notes: HashMap<String, String>,
    pub edits: HashMap<String, String>,
    pub note: String,
    pub new_schedule: String,
    pub new_source: String,
    pub secrets: HashMap<&'static str, Entry>,
}

pub struct Board {
    link: Link,
    store: Box<dyn Store>,
    pub conn: Conn,
    pub screen: Screen,
    pub pref: Preference,
    pub status: Value,
    pub presets: Vec<Value>,
    pub standing: Value,
    pub history: Value,
    pub history_key: Option<String>,
    pub docker: Value,
    pub detail: Option<Value>,
    pub selected_run: Option<String>,
    pub feedback: Option<Feedback>,
    pub confirm: Option<Confirm>,
    pub offer: Option<Offer>,
    pub forms: Forms,
    /// Toasts raised since the shell last took them.
    pub toasts: Vec<Toast>,
    pub in_flight: usize,
    last_start: Option<(String, Value)>,
    want_status: bool,
    /// A `status` request is out: another is not queued behind it (a slow service would pile up).
    status_out: bool,
    last_status: Instant,
    last_docker: Instant,
}

impl Board {
    pub fn new(link: Link, store: Box<dyn Store>) -> Self {
        let long_ago = Instant::now()
            .checked_sub(Duration::from_secs(3600))
            .unwrap_or_else(Instant::now);
        Self {
            link,
            store,
            conn: Conn::Connecting,
            screen: Screen::Runs,
            pref: Preference::System,
            status: Value::Null,
            presets: Vec::new(),
            standing: Value::Null,
            history: Value::Null,
            history_key: None,
            docker: Value::Null,
            detail: None,
            selected_run: None,
            feedback: None,
            confirm: None,
            offer: None,
            forms: Forms::default(),
            toasts: Vec::new(),
            in_flight: 0,
            last_start: None,
            want_status: false,
            status_out: false,
            last_status: long_ago,
            last_docker: long_ago,
        }
    }

    // ---- Reading ---------------------------------------------------------------------------------

    pub fn online(&self) -> bool {
        matches!(self.conn, Conn::Online { .. })
    }

    /// Board may send H-tier commands: connected, and the same build as the service.
    pub fn can_act(&self) -> bool {
        matches!(self.conn, Conn::Online { build_ok: true, .. })
    }

    pub fn preset(&self, id: &str) -> Option<&Value> {
        self.presets.iter().find(|p| p["id"] == id)
    }

    pub fn secrets_set(&self) -> Vec<(&'static news_core::secrets::Secret, bool)> {
        secrets::which_are_set(self.store.as_ref())
    }

    // ---- The loop --------------------------------------------------------------------------------

    /// Take what the link has received and send the refreshes that are due.
    pub fn poll(&mut self) {
        while let Some(m) = self.link.try_recv() {
            self.handle(m);
        }
        let now = Instant::now();
        if self.online() {
            let due = self.want_status
                && now.duration_since(self.last_status) > Duration::from_millis(400);
            if due || now.duration_since(self.last_status) > Duration::from_secs(15) {
                self.refresh_status();
            }
            if self.screen == Screen::Docker
                && now.duration_since(self.last_docker) > Duration::from_secs(8)
            {
                self.refresh_docker();
            }
        }
    }

    fn ask(&mut self, tag: &str, cmd: &str, args: Value) {
        self.in_flight += 1;
        self.link.send(tag, cmd, args);
    }

    pub fn refresh_status(&mut self) {
        if self.status_out {
            // Ask again as soon as the answer is in.
            self.want_status = true;
            return;
        }
        self.want_status = false;
        self.status_out = true;
        self.last_status = Instant::now();
        self.ask("status", "status", json!({}));
        if let Some(run) = self.selected_run.clone() {
            self.ask(&format!("detail:{run}"), "run.show", json!({"run": run}));
        }
    }

    pub fn refresh_docker(&mut self) {
        self.last_docker = Instant::now();
        self.ask("docker", "docker.status", json!({}));
    }

    pub fn refresh_standing(&mut self) {
        self.ask("standing", "standing.get", json!({}));
        let args = self
            .history_key
            .as_ref()
            .map_or_else(|| json!({"limit": 30}), |k| json!({"key": k, "limit": 30}));
        self.ask("history", "standing.history", args);
    }

    fn refresh_all(&mut self) {
        self.ask("presets", "run.presets", json!({}));
        self.refresh_status();
        self.refresh_standing();
        self.refresh_docker();
    }

    pub fn handle(&mut self, m: Msg) {
        match m {
            Msg::Link(LinkState::Online(ping)) => {
                let build_ok = ping["build"] == own_build().as_str();
                let first = !self.online();
                self.conn = Conn::Online { build_ok, ping };
                if !build_ok {
                    self.feedback = Some(Feedback {
                        ok: false,
                        text: "Board and the service are different builds. Restart Board before \
                               sending anything."
                            .to_owned(),
                    });
                }
                if first {
                    self.refresh_all();
                }
            }
            Msg::Link(LinkState::Offline(reason)) => {
                let scm = scm_note();
                self.conn = Conn::Offline { reason, scm };
                self.in_flight = 0;
                self.status_out = false;
            }
            Msg::Event { topic, data, .. } => {
                if let Some(t) = toast::from_event(&topic, &data) {
                    self.toasts.push(t);
                }
                match topic.as_str() {
                    "run" | "gate" | "lineup" | "schedule" | "usage" => self.want_status = true,
                    "standing" => self.refresh_standing(),
                    _ => {}
                }
            }
            Msg::Reply {
                tag, cmd, result, ..
            } => {
                self.in_flight = self.in_flight.saturating_sub(1);
                self.reply(&tag, &cmd, result);
            }
        }
    }

    fn reply(&mut self, tag: &str, cmd: &str, result: Result<Value, ErrorInfo>) {
        if let Some(c) = tag.strip_prefix("act:") {
            return self.acted(c, cmd, result);
        }
        if tag == "status" {
            self.status_out = false;
        }
        let Ok(d) = result else {
            // A refresh that failed leaves the last good data on screen; the banner says why.
            return;
        };
        match tag {
            "status" => self.status = d,
            "presets" => {
                self.presets = d["items"].as_array().cloned().unwrap_or_default();
                if self.forms.preset.is_empty() {
                    self.forms.preset = self
                        .presets
                        .first()
                        .and_then(|p| p["id"].as_str())
                        .unwrap_or_default()
                        .to_owned();
                }
            }
            "standing" => self.standing = d,
            "history" => self.history = d,
            "docker" => self.docker = d,
            t if t
                .strip_prefix("detail:")
                .is_some_and(|run| self.selected_run.as_deref() == Some(run)) =>
            {
                self.detail = Some(d);
            }
            _ => {}
        }
    }

    fn acted(&mut self, _action: &str, cmd: &str, result: Result<Value, ErrorInfo>) {
        match result {
            Ok(d) => {
                self.feedback = Some(Feedback {
                    ok: true,
                    text: d["text"].as_str().unwrap_or("Done.").to_owned(),
                });
                self.offer = None;
                self.confirm = None;
                self.want_status = true;
                match cmd {
                    "standing.set" => {
                        self.forms.edits.clear();
                        self.forms.note.clear();
                        self.refresh_standing();
                    }
                    c if c.starts_with("docker.") => self.refresh_docker(),
                    _ => {}
                }
            }
            Err(e) => {
                if cmd == "run.start"
                    && let (Some(o), Some((preset, args))) =
                        (logic::queue_offer(&e.details), self.last_start.clone())
                {
                    let blocking = o["blocking"]["short_id"].as_str().unwrap_or_default();
                    self.offer = Some(Offer {
                        preset,
                        args,
                        blocking: blocking.to_owned(),
                        text: e.shown().to_owned(),
                    });
                    return;
                }
                if cmd == "standing.set" && e.code == "conflict" {
                    // Someone changed a setting since Board read them: show the new versions.
                    self.refresh_standing();
                }
                self.feedback = Some(Feedback {
                    ok: false,
                    text: e.shown().to_owned(),
                });
            }
        }
    }

    // ---- Opening ---------------------------------------------------------------------------------

    pub fn open(&mut self, t: &OpenTarget) {
        self.screen = t.screen();
        if let Some(run) = t.run() {
            self.select_run(run);
        }
    }

    pub fn select_run(&mut self, short_id: &str) {
        self.selected_run = Some(short_id.to_owned());
        self.detail = None;
        if self.online() {
            self.ask(
                &format!("detail:{short_id}"),
                "run.show",
                json!({"run": short_id}),
            );
        }
    }

    pub fn close_detail(&mut self) {
        self.selected_run = None;
        self.detail = None;
    }

    // ---- Actions (H tier: each is one click) -----------------------------------------------------

    fn send_h(&mut self, cmd: &str, args: Value) {
        if !self.can_act() {
            self.feedback = Some(Feedback {
                ok: false,
                text: if self.online() {
                    "Restart Board: it is not the same build as the service."
                } else {
                    "The service is offline."
                }
                .to_owned(),
            });
            return;
        }
        self.ask(&format!("act:{cmd}"), cmd, args);
    }

    pub fn start_run(&mut self, preset: &str, if_busy: Option<&str>) {
        let Some(decl) = self.preset(preset).cloned() else {
            return;
        };
        match logic::start_args(&decl, &self.forms.args) {
            Err(e) => {
                self.feedback = Some(Feedback { ok: false, text: e });
            }
            Ok(args) => self.start_with(preset, args, if_busy),
        }
    }

    fn start_with(&mut self, preset: &str, args: Value, if_busy: Option<&str>) {
        self.last_start = Some((preset.to_owned(), args.clone()));
        let mut req = json!({"preset": preset, "args": args});
        if let Some(b) = if_busy {
            req["if_busy"] = json!(b);
        }
        self.send_h("run.start", req);
    }

    /// The offer's "Queue it".
    pub fn queue_offered(&mut self) {
        if let Some(o) = self.offer.take() {
            self.start_with(&o.preset, o.args, Some("queue"));
        }
    }

    /// The offer's "Pause the running one and start this".
    pub fn pause_and_start(&mut self) {
        if let Some(o) = self.offer.take() {
            self.send_h(
                "run.start_after_pause",
                json!({"preset": o.preset, "args": o.args, "pause_run": o.blocking}),
            );
        }
    }

    /// Answer an open gate (`gate` is the `gateOpen` Board shows): bound to its hash and round.
    pub fn answer_gate(&mut self, gate: &Value, decision: &str) {
        let key = format!(
            "{}.{}",
            gate["short_id"].as_str().unwrap_or_default(),
            gate["gate"].as_str().unwrap_or_default()
        );
        let note = self.forms.gate_notes.get(&key).cloned().unwrap_or_default();
        match logic::gate_answer(gate, decision, &note) {
            Ok((cmd, args)) => {
                self.forms.gate_notes.remove(&key);
                self.send_h(&cmd, args);
            }
            Err(e) => self.feedback = Some(Feedback { ok: false, text: e }),
        }
    }

    /// Pause, resume or cancel a run. Cancel asks twice.
    pub fn run_control(&mut self, cmd: &str, run: &str, confirmed: bool) {
        if cmd == "run.cancel" && !confirmed {
            self.confirm = Some(Confirm {
                label: format!("Cancel {run}?"),
                cmd: cmd.to_owned(),
                args: json!({"run": run}),
            });
            return;
        }
        self.send_h(cmd, json!({"run": run}));
    }

    pub fn remove_from_lineup(&mut self, run: &str, confirmed: bool) {
        if !confirmed {
            self.confirm = Some(Confirm {
                label: format!("Remove {run} from the Lineup?"),
                cmd: "lineup.cancel".to_owned(),
                args: json!({"run": run}),
            });
            return;
        }
        self.send_h("lineup.cancel", json!({"run": run}));
    }

    /// The second click of a confirmation.
    pub fn confirmed(&mut self) {
        if let Some(c) = self.confirm.take() {
            self.send_h(&c.cmd, c.args);
        }
    }

    /// Save every edited setting as one new Standing version.
    pub fn save_settings(&mut self) {
        let items = self.standing["items"].clone();
        let changes = match logic::standing_changes(&items, &self.forms.edits) {
            Ok(c) => c,
            Err(e) => {
                self.feedback = Some(Feedback { ok: false, text: e });
                return;
            }
        };
        if changes.is_empty() {
            self.feedback = Some(Feedback {
                ok: true,
                text: "Nothing changed.".to_owned(),
            });
            return;
        }
        let base = self.standing["version"].as_i64().unwrap_or(0);
        let mut args = json!({"base_version": base, "changes": changes});
        let note = self.forms.note.trim();
        if !note.is_empty() {
            args["note"] = json!(note);
        }
        self.send_h("standing.set", args);
    }

    pub fn show_history(&mut self, key: Option<String>) {
        self.history_key = key;
        self.refresh_standing();
    }

    pub fn docker_control(&mut self, name: &str, start: bool) {
        let cmd = if start { "docker.start" } else { "docker.stop" };
        self.send_h(cmd, json!({"container": name}));
    }

    /// `service.update`: wait for a safe point, swap the binary, restart.
    pub fn update_service(&mut self) {
        self.send_h("service.update", json!({}));
    }

    // ---- Secrets (Credential Manager, never the pipe) --------------------------------------------

    pub fn save_secret(&mut self, name: &str) {
        let Some(secret) = SECRETS.iter().find(|s| s.name == name) else {
            return;
        };
        let entry = self.forms.secrets.entry(secret.name).or_default();
        let r = secrets::save(self.store.as_ref(), secret, entry);
        self.feedback = Some(match r {
            Ok(text) => Feedback { ok: true, text },
            Err(text) => Feedback { ok: false, text },
        });
    }

    pub fn remove_secret(&mut self, name: &str) {
        let Some(secret) = SECRETS.iter().find(|s| s.name == name) else {
            return;
        };
        self.feedback = Some(match secrets::remove(self.store.as_ref(), secret) {
            Ok(text) => Feedback { ok: true, text },
            Err(text) => Feedback { ok: false, text },
        });
    }

    // ---- The offline banner ----------------------------------------------------------------------

    /// "Start service": ask the Service Control Manager (not the Hotline: there is none yet).
    pub fn start_service(&mut self) {
        self.feedback = Some(match news_win::scm::start(news_win::SERVICE_NAME) {
            Ok(()) => Feedback {
                ok: true,
                text: "Starting the Bureau service.".to_owned(),
            },
            Err(e) => Feedback {
                ok: false,
                text: format!(
                    "Could not start the service: {e}. Run `news service install` if it is not installed."
                ),
            },
        });
    }
}

/// What the Service Control Manager says about the service, for the offline banner.
fn scm_note() -> Option<String> {
    match news_win::scm::status(news_win::SERVICE_NAME) {
        Ok(None) => Some("The service is not installed. Run `news service install`.".to_owned()),
        Ok(Some(s)) => Some(match s.exit_code {
            Some(c) if !s.running => format!(
                "The service is {}. Last error: {}",
                s.state,
                news_win::explain_exit_code(c)
            ),
            _ => format!("The service is {}.", s.state),
        }),
        Err(_) => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::link::{BoxStream, Connect};
    use news_core::NewsError;
    use news_hotline::{BoxFut, Bus, Peer, Refusal, Service};
    use std::sync::{Arc, Mutex};

    /// A Hotline service that answers from a table and records every request it was given.
    struct Fake {
        replies: Mutex<HashMap<String, Result<Value, NewsError>>>,
        seen: Mutex<Vec<(String, Value)>>,
        bus: Bus,
    }

    impl Fake {
        fn new() -> Arc<Self> {
            let f = Arc::new(Self {
                replies: Mutex::default(),
                seen: Mutex::default(),
                bus: Bus::default(),
            });
            f.on("service.ping", Ok(ping()));
            f.on(
                "status",
                Ok(json!({"running": [], "waiting": [], "queued": [], "text": "Quiet."})),
            );
            f.on(
                "run.presets",
                Ok(json!({"items": [
                {"id": "edition", "name": "Edition", "args": [
                    {"name": "pages", "type": "integer", "required": false, "default": null,
                     "min": 1, "max": 500, "doc": "", "choices": null}]}]})),
            );
            f.on(
                "standing.get",
                Ok(
                    json!({"version": 3, "created_at": "2026-11-12T10:00:00Z", "items": [
                {"key": "chief.stall_minutes", "type": "integer", "value": 15, "default": 15,
                 "is_set": false, "doc": "", "choices": null, "min": 1, "max": 10080}]}),
                ),
            );
            f.on("standing.history", Ok(json!({"items": []})));
            f.on(
                "docker.status",
                Ok(json!({"engine": {"reachable": true, "reason": null},
                "containers": [], "text": "Docker is running."})),
            );
            f
        }

        fn on(&self, cmd: &str, r: Result<Value, NewsError>) {
            self.replies.lock().unwrap().insert(cmd.to_owned(), r);
        }

        fn sent(&self, cmd: &str) -> Vec<Value> {
            self.seen
                .lock()
                .unwrap()
                .iter()
                .filter(|(c, _)| c == cmd)
                .map(|(_, a)| a.clone())
                .collect()
        }
    }

    fn ping() -> Value {
        json!({"service": "bureau", "state": "up", "reason": null, "version": news_core::VERSION,
               "build": own_build(), "protocol": 1, "pid": 1, "started_at": "2026-11-12T10:00:00Z",
               "settings_version": 3, "time_zone": "UTC", "text": "Bureau service up"})
    }

    impl Service for Fake {
        fn handle<'a>(
            &'a self,
            cmd: &'a str,
            args: Value,
            _p: &'a Peer,
        ) -> BoxFut<'a, Result<Value, NewsError>> {
            Box::pin(async move {
                self.seen.lock().unwrap().push((cmd.to_owned(), args));
                self.replies
                    .lock()
                    .unwrap()
                    .get(cmd)
                    .cloned()
                    .unwrap_or_else(|| Ok(json!({"text": "ok"})))
            })
        }
        fn ready(&self) -> bool {
            true
        }
        fn refused<'a>(&'a self, cmd: &'a str, _p: &'a Peer, _r: Refusal) -> BoxFut<'a, ()> {
            Box::pin(async move {
                self.seen
                    .lock()
                    .unwrap()
                    .push((format!("refused:{cmd}"), Value::Null));
            })
        }
        fn bus(&self) -> &Bus {
            &self.bus
        }
    }

    struct Duplex {
        svc: Arc<Fake>,
        peer: Peer,
    }

    impl Connect for Duplex {
        fn connect(&self) -> BoxFut<'_, Result<BoxStream, String>> {
            Box::pin(async {
                let (client, server) = tokio::io::duplex(1 << 20);
                let svc: Arc<dyn Service> = self.svc.clone();
                tokio::spawn(news_hotline::serve_conn(server, self.peer.clone(), svc));
                Ok(Box::new(client) as BoxStream)
            })
        }
    }

    struct Offline;
    impl Connect for Offline {
        fn connect(&self) -> BoxFut<'_, Result<BoxStream, String>> {
            Box::pin(async { Err("The Bureau service is not running.".to_owned()) })
        }
    }

    #[derive(Default)]
    struct Mem(Mutex<HashMap<String, String>>);
    impl Store for Mem {
        fn write(&self, t: &str, s: &str) -> std::io::Result<()> {
            self.0.lock().unwrap().insert(t.to_owned(), s.to_owned());
            Ok(())
        }
        fn exists(&self, t: &str) -> bool {
            self.0.lock().unwrap().contains_key(t)
        }
        fn delete(&self, t: &str) -> std::io::Result<()> {
            self.0.lock().unwrap().remove(t);
            Ok(())
        }
    }

    fn board(conn: Arc<dyn Connect>) -> Board {
        Board::new(Link::spawn(conn, || {}), Box::new(Mem::default()))
    }

    fn board_on(svc: &Arc<Fake>, peer: Peer) -> Board {
        board(Arc::new(Duplex {
            svc: svc.clone(),
            peer,
        }))
    }

    /// Poll until `cond` holds (the link runs on its own thread).
    fn wait(b: &mut Board, what: &str, cond: impl Fn(&Board) -> bool) {
        for _ in 0..400 {
            b.poll();
            if cond(b) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!(
            "timed out waiting for {what}: {:?} {:?}",
            b.conn, b.feedback
        );
    }

    const SHA: &str = "9c1e5d7a22bb9c1e5d7a22bb9c1e5d7a22bb9c1e5d7a22bb9c1e5d7a22bbaaaa";

    fn open_gate() -> Value {
        json!({"run_id": "11111111-1111-4111-8111-111111111111", "short_id": "r_51d2",
               "preset": "edition", "gate": "plan", "round": 2, "step": "plan",
               "since": "2026-11-12T16:02:11Z", "subject_sha": SHA, "summary": {"text": "Plan"},
               "preview": null, "actions": ["gate.approve", "gate.changes", "gate.reject"]})
    }

    #[test]
    fn connects_checks_the_build_and_loads_the_first_screens() {
        let svc = Fake::new();
        let mut b = board_on(&svc, Peer::board());
        wait(&mut b, "online with data", |b| {
            b.can_act()
                && !b.presets.is_empty()
                && b.standing["version"] == 3
                && !b.status.is_null()
        });
        assert_eq!(b.forms.preset, "edition");
    }

    #[test]
    fn a_service_of_another_build_blocks_every_action() {
        let svc = Fake::new();
        let mut other = ping();
        other["build"] = json!("0.0.0+other");
        svc.on("service.ping", Ok(other));
        let mut b = board_on(&svc, Peer::board());
        wait(&mut b, "online", |b| b.online());
        assert!(!b.can_act());
        b.docker_control("news-otel", true);
        assert!(b.feedback.as_ref().unwrap().text.contains("Restart Board"));
        assert!(svc.sent("docker.start").is_empty());
    }

    #[test]
    fn a_gate_answer_carries_the_hash_and_round_it_showed_and_changes_needs_a_note() {
        let svc = Fake::new();
        svc.on(
            "gate.approve",
            Ok(json!({"text": "Gate plan of r_51d2 approved; the run is running."})),
        );
        let mut b = board_on(&svc, Peer::board());
        wait(&mut b, "online", |b| b.can_act());
        let gate = open_gate();
        b.answer_gate(&gate, "approve");
        wait(&mut b, "feedback", |b| {
            b.feedback.as_ref().is_some_and(|f| f.ok)
        });
        assert_eq!(
            svc.sent("gate.approve"),
            vec![json!({"run": "r_51d2", "gate": "plan", "round": 2, "subject_sha": SHA})]
        );
        b.answer_gate(&gate, "changes");
        assert!(!b.feedback.as_ref().unwrap().ok, "no note, nothing sent");
        assert!(svc.sent("gate.changes").is_empty());
        b.forms
            .gate_notes
            .insert("r_51d2.plan".into(), "shorter headline".into());
        svc.on("gate.changes", Ok(json!({"text": "Sent back."})));
        b.answer_gate(&gate, "changes");
        wait(&mut b, "changes sent", |_| {
            !svc.sent("gate.changes").is_empty()
        });
        assert_eq!(svc.sent("gate.changes")[0]["note"], "shorter headline");
    }

    #[test]
    fn the_same_actions_from_the_terminal_are_refused_as_gated() {
        let svc = Fake::new();
        let terminal = Peer {
            argv1_is_board: false,
            ..Peer::board()
        };
        // The server loop admits by peer; here the "Board" is a client that is not `news board`.
        let mut b = board_on(&svc, terminal);
        wait(&mut b, "online", |b| b.online());
        b.run_control("run.pause", "r_51d2", true);
        wait(&mut b, "a refusal", |b| b.feedback.is_some());
        let f = b.feedback.clone().unwrap();
        assert!(!f.ok);
        assert!(f.text.contains("only be taken in Board"), "{f:?}");
        assert!(svc.sent("run.pause").is_empty(), "the service never ran it");
        assert!(
            !svc.sent("refused:run.pause").is_empty()
                || svc
                    .seen
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|(c, _)| c.starts_with("refused"))
        );
        // Reads from the terminal still work: the status arrived.
        assert!(!b.status.is_null());
    }

    #[test]
    fn a_busy_heavy_slot_offers_queue_or_pause_and_both_buttons_work() {
        let svc = Fake::new();
        let details = json!({"reason": "queue_offer", "requested": "edition",
                             "blocking": {"run_id": "22222222-2222-4222-8222-222222222222",
                                          "short_id": "r_old1", "preset": "flash", "status": "running", "step": null}});
        svc.on(
            "run.start",
            Err(NewsError::conflict("the heavy slot is busy")
                .with_details(details)
                .with_text("Flash r_old1 is running. Queue it, or pause that one?")),
        );
        let mut b = board_on(&svc, Peer::board());
        wait(&mut b, "online with presets", |b| {
            b.can_act() && !b.presets.is_empty()
        });
        b.forms.args.insert("pages".into(), "40".into());
        b.start_run("edition", None);
        wait(&mut b, "the offer", |b| b.offer.is_some());
        assert_eq!(b.offer.as_ref().unwrap().blocking, "r_old1");
        assert_eq!(
            svc.sent("run.start")[0],
            json!({"preset": "edition", "args": {"pages": 40}})
        );
        svc.on(
            "run.start",
            Ok(json!({"text": "Edition r_new queued, position 1."})),
        );
        b.queue_offered();
        wait(&mut b, "queued", |_| svc.sent("run.start").len() == 2);
        assert_eq!(svc.sent("run.start")[1]["if_busy"], "queue");
        b.offer = Some(Offer {
            preset: "edition".into(),
            args: json!({}),
            blocking: "r_old1".into(),
            text: String::new(),
        });
        b.pause_and_start();
        wait(&mut b, "pause and start", |_| {
            !svc.sent("run.start_after_pause").is_empty()
        });
        assert_eq!(svc.sent("run.start_after_pause")[0]["pause_run"], "r_old1");
    }

    #[test]
    fn cancel_asks_twice_and_one_set_carries_every_edited_setting() {
        let svc = Fake::new();
        let mut b = board_on(&svc, Peer::board());
        wait(&mut b, "online with settings", |b| {
            b.can_act() && b.standing["version"] == 3
        });
        b.run_control("run.cancel", "r_51d2", false);
        assert!(svc.sent("run.cancel").is_empty());
        assert_eq!(b.confirm.as_ref().unwrap().label, "Cancel r_51d2?");
        b.confirmed();
        wait(&mut b, "cancel", |_| !svc.sent("run.cancel").is_empty());
        b.forms
            .edits
            .insert("chief.stall_minutes".into(), "20".into());
        b.forms.note = "longer stall".into();
        b.save_settings();
        wait(&mut b, "set", |_| !svc.sent("standing.set").is_empty());
        assert_eq!(
            svc.sent("standing.set")[0],
            json!({"base_version": 3, "changes": [{"key": "chief.stall_minutes", "value": 20}], "note": "longer stall"})
        );
    }

    #[test]
    fn events_raise_toasts_and_refresh_the_board() {
        let svc = Fake::new();
        let mut b = board_on(&svc, Peer::board());
        wait(&mut b, "online", |b| b.can_act() && !b.status.is_null());
        let before = svc.sent("status").len();
        // Give the event connection a moment to subscribe, then emit a gate event.
        std::thread::sleep(Duration::from_millis(150));
        svc.bus.emit(
            "gate",
            json!({"run_id": "11111111-1111-4111-8111-111111111111", "short_id": "r_51d2",
                   "preset": "edition", "gate": "plan", "round": 1, "state": "open",
                   "subject_sha": SHA, "decision": null,
                   "text": "Edition r_51d2 is waiting for your approval of the plan."}),
        );
        wait(&mut b, "a toast", |b| !b.toasts.is_empty());
        let t = b.toasts.remove(0);
        assert_eq!(t.open.arg(), "gate:r_51d2.plan");
        b.open(&t.open);
        assert_eq!(b.screen, Screen::Runs);
        assert_eq!(b.selected_run.as_deref(), Some("r_51d2"));
        wait(&mut b, "a status refresh", |_| {
            svc.sent("status").len() > before
        });
    }

    #[test]
    fn with_the_service_stopped_board_is_offline_and_sends_nothing() {
        let mut b = board(Arc::new(Offline));
        wait(&mut b, "offline", |b| {
            matches!(b.conn, Conn::Offline { .. })
        });
        let Conn::Offline { reason, .. } = &b.conn else {
            unreachable!()
        };
        assert!(reason.contains("not running"));
        assert!(!b.can_act());
        b.run_control("run.pause", "r_1", true);
        assert!(!b.feedback.as_ref().unwrap().ok);
    }

    #[test]
    fn a_secret_never_reaches_the_hotline_or_a_message() {
        let svc = Fake::new();
        let mut b = board_on(&svc, Peer::board());
        wait(&mut b, "online", |b| b.can_act());
        let sentinel = "sentinel-secret-7f3a9c";
        b.forms
            .secrets
            .entry("write-api-key")
            .or_default()
            .buffer()
            .push_str(sentinel);
        b.save_secret("write-api-key");
        assert!(b.feedback.as_ref().unwrap().ok);
        assert!(!b.feedback.as_ref().unwrap().text.contains(sentinel));
        assert!(
            b.secrets_set()
                .iter()
                .any(|(s, set)| s.name == "write-api-key" && *set)
        );
        // Do some work that sends requests, then look at everything the service ever received.
        b.docker_control("news-otel", false);
        wait(&mut b, "docker.stop", |_| {
            !svc.sent("docker.stop").is_empty()
        });
        let all = format!("{:?}", svc.seen.lock().unwrap());
        assert!(!all.contains(sentinel));
        assert!(!format!("{:?}", b.forms.secrets).contains(sentinel));
        // The Hotline has no command that carries a secret: no contract input mentions one.
        for c in news_hotline::contracts().commands() {
            assert!(!c.name.contains("secret"), "{}", c.name);
        }
    }
}

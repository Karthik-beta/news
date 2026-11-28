//! The Hotline: the Bureau service's control pipe (ADR 0007, ADR 0008 §8, `contracts/hotline/`).
//!
//! This crate is the protocol only, with no Windows calls: frames, the three message shapes, the
//! command table read from the embedded contracts, argument and reply validation, the admission rule
//! for H-tier commands, the event bus, a server loop generic over any byte stream, and the client.
//! `news-win` supplies the pipe and the facts about each peer; `news-service` supplies the handlers.

use chrono::{DateTime, Utc};
use news_core::{Envelope, ErrorCode, NewsError};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::broadcast;

include!(concat!(env!("OUT_DIR"), "/contracts.rs"));

/// The service's pipe.
pub const PIPE_NAME: &str = r"\\.\pipe\news-hotline";
/// Largest frame body: 1 MiB.
pub const MAX_FRAME: usize = 1 << 20;
/// The protocol version (`v`).
pub const PROTOCOL: i64 = 1;
/// A request connection closes after this long idle.
pub const IDLE: Duration = Duration::from_secs(60);
/// A subscriber this far behind gets one `lagged` event.
pub const LAG: usize = 1024;

const BASE: &str = "https://news.karthik.nz/contracts/";

pub type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

// ---- Frames -----------------------------------------------------------------------------------

#[derive(Debug)]
pub enum FrameError {
    /// The peer closed the stream between frames.
    Closed,
    Empty,
    TooLarge(u32),
    Io(std::io::Error),
}

/// A frame: little-endian `u32` length, then the bytes.
pub fn encode(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 4);
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
    out
}

/// Read one frame. Length 0 or above [`MAX_FRAME`] is an error; so is a stream that ends mid-frame.
pub async fn read_frame<R: AsyncRead + Unpin>(r: &mut R) -> Result<Vec<u8>, FrameError> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Err(FrameError::Closed),
        Err(e) => return Err(FrameError::Io(e)),
    }
    let n = u32::from_le_bytes(len);
    if n == 0 {
        return Err(FrameError::Empty);
    }
    if n as usize > MAX_FRAME {
        return Err(FrameError::TooLarge(n));
    }
    let mut body = vec![0u8; n as usize];
    r.read_exact(&mut body).await.map_err(FrameError::Io)?;
    Ok(body)
}

pub async fn write_frame<W: AsyncWrite + Unpin>(w: &mut W, v: &Value) -> std::io::Result<()> {
    let body = serde_json::to_vec(v).map_err(std::io::Error::other)?;
    w.write_all(&encode(&body)).await?;
    w.flush().await
}

// ---- Contracts and the command table ----------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Any same-user client at Medium integrity or above.
    R,
    /// Board only, and never from a process in the service's AI job.
    H,
}

#[derive(Debug, Clone)]
pub struct Command {
    pub name: String,
    pub tier: Tier,
    /// `x-news-status`: the issue whose handler serves it.
    pub issue: String,
}

struct Retriever;

impl jsonschema::Retrieve for Retriever {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let rel = uri
            .as_str()
            .strip_prefix(BASE)
            .ok_or_else(|| format!("not a News contract: {}", uri.as_str()))?;
        let text = CONTRACTS
            .iter()
            .find(|(p, _)| *p == rel)
            .map(|(_, t)| *t)
            .ok_or_else(|| format!("no contract {rel}"))?;
        Ok(serde_json::from_str(text)?)
    }
}

/// The compiled contracts.
pub struct Contracts {
    commands: HashMap<String, Command>,
    validators: HashMap<String, jsonschema::Validator>,
}

fn compile(url: &str) -> Option<jsonschema::Validator> {
    jsonschema::draft202012::options()
        .with_retriever(Retriever)
        .build(&json!({"$ref": url}))
        .map_err(|e| tracing::error!(url, error = %e, "a Hotline contract does not compile"))
        .ok()
}

/// The contracts, compiled once.
pub fn contracts() -> &'static Contracts {
    static C: OnceLock<Contracts> = OnceLock::new();
    C.get_or_init(|| {
        let mut commands = HashMap::new();
        let mut validators = HashMap::new();
        for (path, text) in CONTRACTS {
            let Some(name) = path
                .strip_prefix("hotline/commands/")
                .and_then(|p| p.strip_suffix(".schema.json"))
            else {
                continue;
            };
            let v: Value = serde_json::from_str(text).unwrap_or(Value::Null);
            let tier = if v["x-news-tier"] == "H" {
                Tier::H
            } else {
                Tier::R
            };
            commands.insert(
                name.to_owned(),
                Command {
                    name: name.to_owned(),
                    tier,
                    issue: v["x-news-status"].as_str().unwrap_or("").to_owned(),
                },
            );
            for part in ["input", "output"] {
                if let Some(val) = compile(&format!("{BASE}{path}#/$defs/{part}")) {
                    validators.insert(format!("{name}/{part}"), val);
                }
            }
        }
        for (path, _) in CONTRACTS {
            if let Some(topic) = path
                .strip_prefix("hotline/events/")
                .and_then(|p| p.strip_suffix(".schema.json"))
                && let Some(val) = compile(&format!("{BASE}{path}#/$defs/data"))
            {
                validators.insert(format!("event:{topic}"), val);
            }
        }
        for shape in ["request", "reply", "event"] {
            if let Some(val) = compile(&format!("{BASE}hotline/{shape}.schema.json")) {
                validators.insert(format!("frame:{shape}"), val);
            }
        }
        Contracts {
            commands,
            validators,
        }
    })
}

impl Contracts {
    pub fn command(&self, name: &str) -> Option<&Command> {
        self.commands.get(name)
    }

    pub fn commands(&self) -> impl Iterator<Item = &Command> {
        self.commands.values()
    }

    fn first_error(&self, key: &str, v: &Value) -> Option<String> {
        let val = self.validators.get(key)?;
        val.iter_errors(v)
            .next()
            .map(|e| format!("{e} at {}", e.instance_path()))
    }

    /// Whether every contract compiled (a test checks it).
    pub fn complete(&self) -> bool {
        self.commands.keys().all(|c| {
            self.validators.contains_key(&format!("{c}/input"))
                && self.validators.contains_key(&format!("{c}/output"))
        }) && ["request", "reply", "event"]
            .iter()
            .all(|s| self.validators.contains_key(&format!("frame:{s}")))
    }

    /// `usage` when `args` break the command's `$defs/input`.
    pub fn check_args(&self, cmd: &str, args: &Value) -> Result<(), NewsError> {
        match self.first_error(&format!("{cmd}/input"), args) {
            None => Ok(()),
            Some(e) => Err(NewsError::usage(format!("{cmd}: invalid args: {e}"))
                .with_details(json!({"schema_error": e}))),
        }
    }

    /// The first way `data` breaks the command's `$defs/output`.
    pub fn check_output(&self, cmd: &str, data: &Value) -> Result<(), String> {
        self.first_error(&format!("{cmd}/output"), data)
            .map_or(Ok(()), Err)
    }

    pub fn check_event(&self, topic: &str, data: &Value) -> Result<(), String> {
        self.first_error(&format!("event:{topic}"), data)
            .map_or(Ok(()), Err)
    }

    /// The first way a whole frame breaks `request`, `reply` or `event`.
    pub fn check_frame(&self, shape: &str, v: &Value) -> Result<(), String> {
        self.first_error(&format!("frame:{shape}"), v)
            .map_or(Ok(()), Err)
    }
}

// ---- Admission ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Integrity {
    Untrusted,
    Low,
    Medium,
    High,
    System,
}

/// What the server knows about a connected client (read by `news-win` from the kernel).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    pub pid: u32,
    pub same_user: bool,
    pub integrity: Integrity,
    /// The client's image path is the installed, admin-only `%ProgramFiles%\News\news.exe`.
    pub image_is_ours: bool,
    /// argv[1] is `board` (a hint, not a proof).
    pub argv1_is_board: bool,
    /// The client is in the service's root marker job (spawned for a run).
    pub in_marker_job: bool,
    pub image: Option<String>,
}

impl Peer {
    /// A trusted Board peer (tests and in-process callers).
    pub fn board() -> Self {
        Self {
            pid: std::process::id(),
            same_user: true,
            integrity: Integrity::Medium,
            image_is_ours: true,
            argv1_is_board: true,
            in_marker_job: false,
            image: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    WrongUser,
    LowIntegrity,
    InAiJob,
    NotOurImage,
    NotBoard,
}

impl Refusal {
    pub fn as_str(self) -> &'static str {
        match self {
            Refusal::WrongUser => "wrong_user",
            Refusal::LowIntegrity => "low_integrity",
            Refusal::InAiJob => "in_ai_job",
            Refusal::NotOurImage => "not_our_image",
            Refusal::NotBoard => "not_board",
        }
    }
}

/// The admission rule (ADR 0008 §8.2). R: any same-user peer at Medium or above (spawned processes
/// included). H: also not in the marker job, our own image, and argv[1] `board`, checked in that order.
pub fn admit(tier: Tier, p: &Peer) -> Result<(), Refusal> {
    if !p.same_user {
        return Err(Refusal::WrongUser);
    }
    if p.integrity < Integrity::Medium {
        return Err(Refusal::LowIntegrity);
    }
    if tier == Tier::R {
        return Ok(());
    }
    if p.in_marker_job {
        return Err(Refusal::InAiJob);
    }
    if !p.image_is_ours {
        return Err(Refusal::NotOurImage);
    }
    if !p.argv1_is_board {
        return Err(Refusal::NotBoard);
    }
    Ok(())
}

// ---- Events ------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub seq: u64,
    pub topic: String,
    pub at: DateTime<Utc>,
    pub data: Value,
}

impl Event {
    pub fn frame(&self) -> Value {
        json!({"v": PROTOCOL, "event": self.topic, "seq": self.seq, "at": self.at, "data": self.data})
    }
}

/// The event bus: `seq` counts from 1 per service process.
pub struct Bus {
    tx: broadcast::Sender<Event>,
    seq: AtomicU64,
}

impl Default for Bus {
    fn default() -> Self {
        Self {
            tx: broadcast::channel(LAG).0,
            seq: AtomicU64::new(0),
        }
    }
}

impl Bus {
    pub fn emit(&self, topic: &str, data: Value) {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = self.tx.send(Event {
            seq,
            topic: topic.to_owned(),
            at: Utc::now(),
            data,
        });
    }

    pub fn subscribe(&self) -> (u64, broadcast::Receiver<Event>) {
        (self.seq.load(Ordering::SeqCst), self.tx.subscribe())
    }
}

// ---- The server loop ---------------------------------------------------------------------------

/// What the server loop needs from the service.
pub trait Service: Send + Sync + 'static {
    /// Run a command already admitted and validated; the result is the envelope's `data`.
    fn handle<'a>(
        &'a self,
        cmd: &'a str,
        args: Value,
        peer: &'a Peer,
    ) -> BoxFut<'a, Result<Value, NewsError>>;
    /// The service is `up` (before that only `service.ping` is answered).
    fn ready(&self) -> bool;
    /// An H-tier command was refused: log it and record it (`hotline_refusals`).
    fn refused<'a>(&'a self, cmd: &'a str, peer: &'a Peer, refusal: Refusal) -> BoxFut<'a, ()>;
    fn bus(&self) -> &Bus;
}

fn reply(id: Value, env: &Envelope) -> Value {
    json!({"v": PROTOCOL, "id": id, "envelope": serde_json::to_value(env).unwrap_or(Value::Null)})
}

fn err_reply(id: Value, e: NewsError) -> Value {
    reply(id, &Envelope::err(e))
}

/// Serve one connection until it closes, idles out, breaks the protocol, or (for a subscription) the
/// peer stops reading. A peer under another user or below Medium integrity is dropped at once,
/// without a reply.
pub async fn serve_conn<S>(mut stream: S, peer: Peer, svc: Arc<dyn Service>)
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    if let Err(r @ (Refusal::WrongUser | Refusal::LowIntegrity)) = admit(Tier::R, &peer) {
        tracing::warn!(
            pid = peer.pid,
            refusal = r.as_str(),
            "Hotline client refused at connect"
        );
        return;
    }
    let c = contracts();
    loop {
        let body = match tokio::time::timeout(IDLE, read_frame(&mut stream)).await {
            Err(_) | Ok(Err(FrameError::Closed)) | Ok(Err(FrameError::Io(_))) => return,
            Ok(Err(e @ (FrameError::Empty | FrameError::TooLarge(_)))) => {
                let _ = write_frame(
                    &mut stream,
                    &err_reply(Value::Null, NewsError::usage(format!("bad frame: {e:?}"))),
                )
                .await;
                return;
            }
            Ok(Ok(b)) => b,
        };
        let Ok(req) = serde_json::from_slice::<Value>(&body) else {
            let _ = write_frame(
                &mut stream,
                &err_reply(Value::Null, NewsError::usage("the frame is not JSON")),
            )
            .await;
            return;
        };
        let id = req
            .get("id")
            .filter(|i| i.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 64))
            .cloned()
            .unwrap_or(Value::Null);
        if req["v"] != PROTOCOL {
            let e = NewsError::usage("unsupported protocol version")
                .with_details(json!({"supported": [PROTOCOL]}));
            let _ = write_frame(&mut stream, &err_reply(id, e)).await;
            continue;
        }
        let cmd_name = req["cmd"].as_str().unwrap_or_default().to_owned();
        let Some(cmd) = c.command(&cmd_name) else {
            let _ = write_frame(
                &mut stream,
                &err_reply(
                    id,
                    NewsError::usage(format!("unknown command {cmd_name:?}")),
                ),
            )
            .await;
            continue;
        };
        let args = req.get("args").cloned().unwrap_or_else(|| json!({}));
        if let Err(e) = c.check_args(&cmd_name, &args) {
            let _ = write_frame(&mut stream, &err_reply(id, e)).await;
            continue;
        }
        if cmd.tier == Tier::H
            && let Err(r) = admit(Tier::H, &peer)
        {
            svc.refused(&cmd_name, &peer, r).await;
            let e = NewsError::gated(format!("{cmd_name} is accepted only from Board"))
                .with_details(json!({"refusal": r.as_str()}))
                .with_text("This action can only be taken in Board.");
            let _ = write_frame(&mut stream, &err_reply(id, e)).await;
            continue;
        }
        if !svc.ready() && cmd_name != "service.ping" {
            let e = NewsError::upstream("the Bureau service is not up yet").with_text(
                "The Bureau service is starting or waiting for the database; try again shortly.",
            );
            let _ = write_frame(&mut stream, &err_reply(id, e)).await;
            continue;
        }
        if cmd_name == "subscribe" {
            let topics: Vec<String> = args["topics"]
                .as_array()
                .map(|t| {
                    t.iter()
                        .filter_map(|x| x.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            let (seq, mut rx) = svc.bus().subscribe();
            let ok = Envelope::ok(json!({"topics": topics, "seq": seq}));
            if write_frame(&mut stream, &reply(id, &ok)).await.is_err() {
                return;
            }
            loop {
                let ev = match rx.recv().await {
                    Ok(ev) => ev,
                    Err(broadcast::error::RecvError::Lagged(n)) => Event {
                        seq: 0,
                        topic: "lagged".into(),
                        at: Utc::now(),
                        data: json!({"missed": n}),
                    },
                    Err(broadcast::error::RecvError::Closed) => return,
                };
                if ev.topic != "lagged" && !topics.contains(&ev.topic) {
                    continue;
                }
                if write_frame(&mut stream, &ev.frame()).await.is_err() {
                    return;
                }
            }
        }
        let env = match svc.handle(&cmd_name, args, &peer).await {
            Ok(data) => Envelope::ok(data),
            Err(e) => Envelope::err(e),
        };
        if write_frame(&mut stream, &reply(id, &env)).await.is_err() {
            return;
        }
    }
}

// ---- The client --------------------------------------------------------------------------------

/// Send one request and read its reply's envelope (the CLI and Board).
pub async fn call<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    id: &str,
    cmd: &str,
    args: Value,
) -> Result<Value, NewsError> {
    let req = json!({"v": PROTOCOL, "id": id, "cmd": cmd, "args": args});
    write_frame(stream, &req)
        .await
        .map_err(|e| NewsError::upstream(format!("Hotline write failed: {e}")))?;
    let body = read_frame(stream)
        .await
        .map_err(|e| NewsError::upstream(format!("Hotline read failed: {e:?}")))?;
    let v: Value = serde_json::from_slice(&body)
        .map_err(|e| NewsError::upstream(format!("Hotline reply is not JSON: {e}")))?;
    Ok(v["envelope"].clone())
}

/// The envelope's error code, if it failed.
pub fn error_code(envelope: &Value) -> Option<ErrorCode> {
    serde_json::from_value(envelope["error"]["code"].clone()).ok()
}

/// Where the server listens: the real pipe, or (tests only) a unique test pipe.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub pipe: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            pipe: PIPE_NAME.to_owned(),
        }
    }
}

impl ServerConfig {
    #[cfg(any(test, feature = "test-support"))]
    pub fn for_tests() -> Self {
        Self {
            pipe: format!(r"\\.\pipe\news-hotline-test-{}", uuid::Uuid::new_v4()),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn every_contract_compiles_and_registers_its_tier() {
        let c = contracts();
        assert!(c.complete());
        assert_eq!(c.commands().count(), 28);
        for (name, tier) in [
            ("status", Tier::R),
            ("standing.get", Tier::R),
            ("run.start", Tier::H),
            ("standing.set", Tier::H),
            ("lineup.cancel", Tier::H),
            ("docker.start", Tier::H),
            ("huddle.answer", Tier::H),
            ("huddle.questions", Tier::R),
            ("form.submit", Tier::H),
            ("home.rollback", Tier::H),
        ] {
            assert_eq!(c.command(name).unwrap().tier, tier, "{name}");
        }
        assert!(
            c.check_args("run.start", &json!({"preset": "edition", "args": {}}))
                .is_ok()
        );
        let e = c
            .check_args("run.start", &json!({"preset": "nope", "args": {}}))
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::Usage);
    }

    #[test]
    fn admission_table() {
        let base = Peer::board();
        for same_user in [false, true] {
            for integrity in [
                Integrity::Untrusted,
                Integrity::Low,
                Integrity::Medium,
                Integrity::High,
            ] {
                for image in [false, true] {
                    for board in [false, true] {
                        for job in [false, true] {
                            let p = Peer {
                                same_user,
                                integrity,
                                image_is_ours: image,
                                argv1_is_board: board,
                                in_marker_job: job,
                                ..base.clone()
                            };
                            let base_ok = same_user && integrity >= Integrity::Medium;
                            assert_eq!(admit(Tier::R, &p).is_ok(), base_ok, "{p:?}");
                            let h = admit(Tier::H, &p);
                            let want = if !same_user {
                                Err(Refusal::WrongUser)
                            } else if integrity < Integrity::Medium {
                                Err(Refusal::LowIntegrity)
                            } else if job {
                                Err(Refusal::InAiJob)
                            } else if !image {
                                Err(Refusal::NotOurImage)
                            } else if !board {
                                Err(Refusal::NotBoard)
                            } else {
                                Ok(())
                            };
                            assert_eq!(h, want, "{p:?}");
                        }
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn frames_round_trip_and_bad_lengths_fail() {
        for body in [b"{}".to_vec(), vec![b'x'; MAX_FRAME]] {
            let enc = encode(&body);
            let mut r = &enc[..];
            assert_eq!(read_frame(&mut r).await.unwrap(), body);
        }
        let mut r = &0u32.to_le_bytes()[..];
        assert!(matches!(read_frame(&mut r).await, Err(FrameError::Empty)));
        let big = ((MAX_FRAME + 1) as u32).to_le_bytes();
        let mut r = &big[..];
        assert!(matches!(
            read_frame(&mut r).await,
            Err(FrameError::TooLarge(_))
        ));
        let mut trunc = encode(b"{\"a\":1}");
        trunc.truncate(6);
        let mut r = &trunc[..];
        assert!(matches!(read_frame(&mut r).await, Err(FrameError::Io(_))));
        let mut r = &[][..];
        assert!(matches!(read_frame(&mut r).await, Err(FrameError::Closed)));
    }

    struct Fake(Bus);
    impl Service for Fake {
        fn handle<'a>(
            &'a self,
            cmd: &'a str,
            _: Value,
            _: &'a Peer,
        ) -> BoxFut<'a, Result<Value, NewsError>> {
            Box::pin(async move {
                match cmd {
                    "lineup.cancel" => Err(NewsError::not_found("no such run")),
                    _ => Ok(json!({"items": []})),
                }
            })
        }
        fn ready(&self) -> bool {
            true
        }
        fn refused<'a>(&'a self, _: &'a str, _: &'a Peer, _: Refusal) -> BoxFut<'a, ()> {
            Box::pin(async {})
        }
        fn bus(&self) -> &Bus {
            &self.0
        }
    }

    async fn round_trip(peer: Peer, cmd: &str, args: Value) -> Value {
        let (mut client, server) = tokio::io::duplex(1 << 21);
        let svc: Arc<dyn Service> = Arc::new(Fake(Bus::default()));
        let h = tokio::spawn(serve_conn(server, peer, svc));
        let env = call(&mut client, "t-1", cmd, args).await.unwrap();
        drop(client);
        let _ = h.await;
        env
    }

    #[tokio::test]
    async fn h_tier_commands_from_anything_but_board_are_gated() {
        let cli = Peer {
            argv1_is_board: false,
            ..Peer::board()
        };
        let env = round_trip(cli.clone(), "lineup.cancel", json!({"run": "r_abcd1234"})).await;
        assert_eq!(env["error"]["code"], "gated");
        assert_eq!(env["error"]["details"]["refusal"], "not_board");
        let marked = Peer {
            in_marker_job: true,
            ..Peer::board()
        };
        let env = round_trip(marked, "lineup.cancel", json!({"run": "r_abcd1234"})).await;
        assert_eq!(env["error"]["details"]["refusal"], "in_ai_job");
        // A read from the terminal is fine; the same H command from Board reaches the handler.
        let env = round_trip(cli, "run.presets", json!({})).await;
        assert_eq!(env["ok"], true);
        let env = round_trip(Peer::board(), "lineup.cancel", json!({"run": "r_abcd1234"})).await;
        assert_eq!(env["error"]["code"], "not_found");
        let env = round_trip(Peer::board(), "no.such", json!({})).await;
        assert_eq!(env["error"]["code"], "usage");
        let reply = json!({"v": 1, "id": "t-1", "envelope": env});
        contracts().check_frame("reply", &reply).unwrap();
    }

    #[tokio::test]
    async fn form_submit_is_board_only_and_its_answer_is_checked_against_the_form() {
        let args = json!({"run": "r_abcd1234", "form": "huddle.calibration", "round": 1,
                          "form_sha": "0".repeat(64),
                          "answer": {"labels": [{"a": 1, "b": 2, "winner": "a"}], "search_console": "fine"}});
        let terminal = Peer {
            argv1_is_board: false,
            ..Peer::board()
        };
        let env = round_trip(terminal, "form.submit", args.clone()).await;
        assert_eq!(env["error"]["details"]["refusal"], "not_board", "{env}");
        assert_eq!(error_code(&env).map(ErrorCode::exit_code), Some(7));
        // From Board, an answer the form's schema refuses never reaches the handler.
        let mut tie = args.clone();
        tie["answer"]["labels"][0]["winner"] = json!("tie");
        let mut extra = args.clone();
        extra["answer"]["mood"] = json!("good");
        for bad in [tie, extra] {
            let env = round_trip(Peer::board(), "form.submit", bad).await;
            assert_eq!(env["error"]["code"], "usage", "{env}");
        }
        let env = round_trip(Peer::board(), "form.submit", args).await;
        assert_eq!(env["ok"], true, "{env}");
    }

    #[tokio::test]
    async fn another_user_is_dropped_without_a_reply() {
        let (mut client, server) = tokio::io::duplex(1024);
        let svc: Arc<dyn Service> = Arc::new(Fake(Bus::default()));
        let peer = Peer {
            same_user: false,
            ..Peer::board()
        };
        tokio::spawn(serve_conn(server, peer, svc));
        let r = call(&mut client, "t-1", "status", json!({})).await;
        assert!(r.is_err());
    }
}

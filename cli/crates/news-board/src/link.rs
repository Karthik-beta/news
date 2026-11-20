//! Board's one connection to the world: the Hotline (ADR 0007 decision 8). A background thread owns
//! a small Tokio runtime, keeps a request connection and an event connection to the service, and
//! hands the UI plain messages over a channel. Board opens no listener and talks to nothing else:
//! the only way to act is a request on the pipe, sent when Karthik clicks.
//!
//! The transport is a [`Connect`], so tests run the same code against an in-memory server.

use news_hotline::{BoxFut, FrameError, call, read_frame};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::mpsc as std_mpsc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}
pub type BoxStream = Box<dyn Stream>;

/// Opens a connection to the Hotline. `Err` is the reason, shown in the offline banner.
pub trait Connect: Send + Sync + 'static {
    fn connect(&self) -> BoxFut<'_, Result<BoxStream, String>>;
}

/// The real pipe, after checking that its server is the installed service.
#[cfg(windows)]
pub struct PipeConnect;

#[cfg(windows)]
impl Connect for PipeConnect {
    fn connect(&self) -> BoxFut<'_, Result<BoxStream, String>> {
        Box::pin(async {
            let mut client = None;
            for _ in 0..10 {
                match news_win::connect_client(news_hotline::PIPE_NAME) {
                    Ok(c) => {
                        client = Some(c);
                        break;
                    }
                    // ERROR_PIPE_BUSY: every instance is serving someone; try again in a moment.
                    Err(e) if e.raw_os_error() == Some(231) => {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        return Err("The Bureau service is not running.".to_owned());
                    }
                    Err(e) => return Err(format!("Cannot open the Hotline: {e}")),
                }
            }
            let client = client.ok_or("The Hotline is busy.")?;
            let installed = news_win::install_paths()
                .map_err(|e| format!("Cannot find the install folder: {e}"))?
                .exe;
            let server = news_win::server_image(&client)
                .map_err(|e| format!("Cannot identify the Hotline's server: {e}"))?;
            if !news_win::same_image(&server, &installed.to_string_lossy()) {
                return Err(
                    "Something else is serving the Hotline: it is not the installed news.exe. \
                     Board sent nothing."
                        .to_owned(),
                );
            }
            Ok(Box::new(client) as BoxStream)
        })
    }
}

/// What the UI asks for.
#[derive(Debug, Clone)]
pub struct Request {
    pub id: u64,
    /// The UI's label for what this was for (`status`, `run:r_51d2`, `act:gate.approve`).
    pub tag: String,
    pub cmd: String,
    pub args: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ErrorInfo {
    pub code: String,
    pub message: String,
    /// Code-rendered text for Karthik, when the service gave one.
    pub text: Option<String>,
    pub details: Value,
}

impl ErrorInfo {
    pub fn offline(message: &str) -> Self {
        Self {
            code: "upstream_unavailable".to_owned(),
            message: message.to_owned(),
            text: Some(message.to_owned()),
            details: Value::Null,
        }
    }

    /// What to show Karthik: the service's own text first.
    pub fn shown(&self) -> &str {
        self.text.as_deref().unwrap_or(&self.message)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkState {
    Offline(String),
    /// The `service.ping` data of the connected service.
    Online(Value),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    Link(LinkState),
    Reply {
        id: u64,
        tag: String,
        cmd: String,
        result: Result<Value, ErrorInfo>,
    },
    Event {
        topic: String,
        at: String,
        data: Value,
    },
}

/// The topics Board subscribes to: everything it raises toasts or refreshes from.
pub const TOPICS: &[&str] = &[
    "run", "gate", "lineup", "schedule", "usage", "standing", "service", "source", "huddle",
];

const CALL_TIMEOUT: Duration = Duration::from_secs(60);
const RETRY: Duration = Duration::from_secs(2);

/// The UI's end of the link.
pub struct Link {
    tx: UnboundedSender<Request>,
    rx: std_mpsc::Receiver<Msg>,
    next: u64,
}

impl Link {
    /// Start the worker thread. `wake` is called after every message (the UI repaints).
    pub fn spawn(conn: Arc<dyn Connect>, wake: impl Fn() + Send + Sync + 'static) -> Link {
        let (tx, req_rx) = unbounded_channel();
        let (out, rx) = std_mpsc::channel();
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
        std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            rt.block_on(worker(conn, req_rx, Sink { out, wake }));
        });
        Link { tx, rx, next: 1 }
    }

    /// Send a request; the reply arrives as a [`Msg::Reply`] with the returned id.
    pub fn send(&mut self, tag: &str, cmd: &str, args: Value) -> u64 {
        let id = self.next;
        self.next += 1;
        let _ = self.tx.send(Request {
            id,
            tag: tag.to_owned(),
            cmd: cmd.to_owned(),
            args,
        });
        id
    }

    pub fn try_recv(&self) -> Option<Msg> {
        self.rx.try_recv().ok()
    }
}

#[derive(Clone)]
struct Sink {
    out: std_mpsc::Sender<Msg>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Sink {
    fn send(&self, m: Msg) {
        let _ = self.out.send(m);
        (self.wake)();
    }
}

fn error_of(env: &Value) -> ErrorInfo {
    let e = &env["error"];
    ErrorInfo {
        code: e["code"].as_str().unwrap_or("internal").to_owned(),
        message: e["message"]
            .as_str()
            .unwrap_or("the service sent no message")
            .to_owned(),
        text: e["text"].as_str().map(str::to_owned),
        details: e["details"].clone(),
    }
}

fn result_of(env: &Value) -> Result<Value, ErrorInfo> {
    if env["ok"] == true {
        Ok(env["data"].clone())
    } else {
        Err(error_of(env))
    }
}

async fn worker(conn: Arc<dyn Connect>, mut rx: UnboundedReceiver<Request>, sink: Sink) {
    loop {
        let mut stream = match conn.connect().await {
            Ok(s) => s,
            Err(reason) => {
                sink.send(Msg::Link(LinkState::Offline(reason.clone())));
                // Answer what was asked while offline, so the UI does not wait for it.
                tokio::select! {
                    () = tokio::time::sleep(RETRY) => {}
                    r = rx.recv() => match r {
                        None => return,
                        Some(r) => sink.send(Msg::Reply {
                            id: r.id, tag: r.tag, cmd: r.cmd,
                            result: Err(ErrorInfo::offline(&reason)),
                        }),
                    },
                }
                continue;
            }
        };
        match call(&mut stream, "b-ping", "service.ping", json!({})).await {
            Ok(env) if env["ok"] == true => {
                sink.send(Msg::Link(LinkState::Online(env["data"].clone())));
            }
            Ok(env) => {
                sink.send(Msg::Link(LinkState::Offline(
                    error_of(&env).shown().to_owned(),
                )));
                tokio::time::sleep(RETRY).await;
                continue;
            }
            Err(e) => {
                sink.send(Msg::Link(LinkState::Offline(e.message)));
                tokio::time::sleep(RETRY).await;
                continue;
            }
        }
        let mut events = tokio::spawn(event_task(conn.clone(), sink.clone()));
        loop {
            tokio::select! {
                r = rx.recv() => {
                    let Some(r) = r else {
                        events.abort();
                        return;
                    };
                    let res = tokio::time::timeout(
                        CALL_TIMEOUT,
                        call(&mut stream, &format!("b-{}", r.id), &r.cmd, r.args.clone()),
                    ).await;
                    match res {
                        Ok(Ok(env)) => sink.send(Msg::Reply {
                            id: r.id, tag: r.tag, cmd: r.cmd, result: result_of(&env),
                        }),
                        Ok(Err(e)) => {
                            sink.send(Msg::Reply {
                                id: r.id, tag: r.tag, cmd: r.cmd,
                                result: Err(ErrorInfo::offline(&e.message)),
                            });
                            break;
                        }
                        Err(_) => {
                            sink.send(Msg::Reply {
                                id: r.id, tag: r.tag, cmd: r.cmd,
                                result: Err(ErrorInfo::offline("The service did not answer in time.")),
                            });
                            break;
                        }
                    }
                }
                _ = &mut events => break,
            }
        }
        events.abort();
        sink.send(Msg::Link(LinkState::Offline(
            "The connection to the service ended.".to_owned(),
        )));
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// A second connection that subscribes and forwards event frames until it breaks.
async fn event_task(conn: Arc<dyn Connect>, sink: Sink) {
    let Ok(mut s) = conn.connect().await else {
        return;
    };
    let topics: Vec<&str> = TOPICS.to_vec();
    match call(&mut s, "b-sub", "subscribe", json!({"topics": topics})).await {
        Ok(env) if env["ok"] == true => {}
        _ => return,
    }
    loop {
        let body = match read_frame(&mut s).await {
            Ok(b) => b,
            Err(
                FrameError::Closed
                | FrameError::Io(_)
                | FrameError::Empty
                | FrameError::TooLarge(_),
            ) => {
                return;
            }
        };
        let Ok(v) = serde_json::from_slice::<Value>(&body) else {
            continue;
        };
        if let Some(topic) = v["event"].as_str() {
            sink.send(Msg::Event {
                topic: topic.to_owned(),
                at: v["at"].as_str().unwrap_or_default().to_owned(),
                data: v["data"].clone(),
            });
        }
    }
}

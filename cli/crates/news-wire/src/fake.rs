//! A scripted in-memory [`Transport`] for tests and fixture replays. It never opens a socket: requests
//! are answered from routes keyed by URL prefix, and every request is recorded.

use std::collections::VecDeque;
use std::sync::Mutex;

use crate::pace::BoxFut;
use crate::runner::{Body, HttpRequest, HttpResponse, Transport, TransportError};

#[derive(Debug, Clone)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Size of each body chunk, to exercise streaming.
    pub chunk: usize,
}

impl Reply {
    pub fn ok(content_type: &str, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            headers: vec![("Content-Type".into(), content_type.into())],
            body: body.into(),
            chunk: 4096,
        }
    }

    pub fn status(status: u16) -> Self {
        Self {
            status,
            headers: vec![],
            body: vec![],
            chunk: 4096,
        }
    }

    pub fn header(mut self, k: &str, v: &str) -> Self {
        self.headers.push((k.into(), v.into()));
        self
    }
}

#[derive(Default)]
pub struct FakeTransport {
    /// `(url prefix, replies)`: the first matching prefix answers; its replies are used in order and
    /// the last one repeats.
    routes: Mutex<Vec<(String, VecDeque<Reply>)>>,
    pub requests: Mutex<Vec<HttpRequest>>,
}

impl FakeTransport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn route(&self, prefix: &str, replies: Vec<Reply>) -> &Self {
        if let Ok(mut r) = self.routes.lock() {
            r.push((prefix.to_owned(), replies.into()));
        }
        self
    }

    pub fn urls(&self) -> Vec<String> {
        self.requests
            .lock()
            .map(|r| r.iter().map(|q| q.url.clone()).collect())
            .unwrap_or_default()
    }
}

struct ChunkedBody {
    chunks: VecDeque<Vec<u8>>,
}

impl Body for ChunkedBody {
    fn chunk(&mut self) -> BoxFut<'_, Result<Option<Vec<u8>>, TransportError>> {
        let next = self.chunks.pop_front();
        Box::pin(async move { Ok(next) })
    }
}

impl Transport for FakeTransport {
    fn get(&self, req: HttpRequest) -> BoxFut<'_, Result<HttpResponse, TransportError>> {
        let reply = self.routes.lock().ok().and_then(|mut routes| {
            let (_, q) = routes
                .iter_mut()
                .find(|(p, _)| req.url.starts_with(p.as_str()))?;
            if q.len() > 1 {
                q.pop_front()
            } else {
                q.front().cloned()
            }
        });
        if let Ok(mut r) = self.requests.lock() {
            r.push(req);
        }
        Box::pin(async move {
            let reply = reply.ok_or_else(|| TransportError {
                message: "no fake route".into(),
            })?;
            let chunks = reply
                .body
                .chunks(reply.chunk.max(1))
                .map(<[u8]>::to_vec)
                .collect();
            Ok(HttpResponse {
                status: reply.status,
                headers: reply.headers,
                body: Box::new(ChunkedBody { chunks }),
            })
        })
    }
}

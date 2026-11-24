//! A scripted in-memory write API for tests: no sockets. It checks the key's shape, applies each write
//! once per `Idempotency-Key` and replays the stored answer for a repeat, as the real API does
//! (`contracts/cloud/write-api.openapi.json`); a script injects failures first.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use news_wire::pace::BoxFut;
use serde_json::{Value, json};

use crate::api::{Api, ApiError, ApiRequest, ApiResponse};

/// What happens to the next request, before the API behaves normally.
#[derive(Debug, Clone)]
pub enum Fault {
    /// Answer with this status and error code; nothing is applied.
    Status(u16, &'static str, Option<&'static str>),
    /// Apply the write, then drop the answer (the connection dies): the outcome is unknown to the caller.
    LoseAnswer,
    /// Drop the request before it is applied.
    Unreachable,
}

#[derive(Default)]
struct State {
    script: VecDeque<Fault>,
    /// key -> (body length as a fingerprint, stored answer)
    stored: HashMap<String, (usize, Value)>,
    requests: Vec<ApiRequest>,
    /// Keys whose write was applied, in order: a replay never adds to it.
    applied: Vec<String>,
}

#[derive(Default)]
pub struct FakeApi(Mutex<State>);

fn key_ok(k: &str) -> bool {
    (8..=255).contains(&k.len())
        && k.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b":._-".contains(&b))
}

fn error(status: u16, code: &str, retry_after: Option<&str>) -> ApiResponse {
    ApiResponse {
        status,
        retry_after: retry_after.map(str::to_owned),
        body: json!({"error": {"code": code, "message": format!("fake {code}")}})
            .to_string()
            .into_bytes(),
    }
}

impl FakeApi {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn script(&self, faults: impl IntoIterator<Item = Fault>) -> &Self {
        if let Ok(mut s) = self.0.lock() {
            s.script.extend(faults);
        }
        self
    }

    pub fn requests(&self) -> Vec<ApiRequest> {
        self.0
            .lock()
            .map(|s| s.requests.clone())
            .unwrap_or_default()
    }

    /// Idempotency keys of the writes that were applied.
    pub fn applied(&self) -> Vec<String> {
        self.0.lock().map(|s| s.applied.clone()).unwrap_or_default()
    }

    fn handle(&self, req: ApiRequest) -> Result<ApiResponse, ApiError> {
        let mut s = self.0.lock().map_err(|_| ApiError("poisoned".into()))?;
        s.requests.push(req.clone());
        let fault = s.script.pop_front();
        if let Some(Fault::Status(code, name, retry)) = fault {
            return Ok(error(code, name, retry));
        }
        if matches!(fault, Some(Fault::Unreachable)) {
            return Err(ApiError("connection refused".into()));
        }
        if !key_ok(&req.idempotency_key) {
            return Ok(error(400, "idempotency_key_invalid", None));
        }
        let fingerprint = req.body.len();
        let answer = match s.stored.get(&req.idempotency_key) {
            Some((f, _)) if *f != fingerprint => {
                return Ok(error(409, "idempotency_key_reused", None));
            }
            Some((_, stored)) => stored.clone(),
            None => {
                let body: Value = match serde_json::from_slice(&req.body) {
                    Ok(b) => b,
                    Err(_) => return Ok(error(400, "invalid_json", None)),
                };
                let page = req.path.split('/').nth(3).unwrap_or_default().to_owned();
                let answer = if req.path.ends_with("/withdraw") {
                    json!({"page_id": page, "version": 1, "status": "withdrawn", "index_status": "noindex", "purged": []})
                } else if req.path.ends_with("/index") {
                    json!({"page_id": page, "version": 1, "status": "published",
                           "index_status": body["index_status"], "purged": []})
                } else if req.path.starts_with("/v1/pages/") {
                    json!({"page_id": page, "version": body["manifest"]["version"],
                           "url": format!("/2026/11/{page}"), "status": "published", "purged": []})
                } else {
                    let n = body["nodes"]
                        .as_array()
                        .or(body["redirects"].as_array())
                        .map_or(0, Vec::len);
                    json!({"written": n, "purged": []})
                };
                s.stored
                    .insert(req.idempotency_key.clone(), (fingerprint, answer.clone()));
                s.applied.push(req.idempotency_key.clone());
                answer
            }
        };
        if matches!(fault, Some(Fault::LoseAnswer)) {
            return Err(ApiError("connection reset".into()));
        }
        Ok(ApiResponse {
            status: 200,
            retry_after: None,
            body: answer.to_string().into_bytes(),
        })
    }
}

impl Api for FakeApi {
    fn send(&self, req: ApiRequest) -> BoxFut<'_, Result<ApiResponse, ApiError>> {
        let r = self.handle(req);
        Box::pin(async move { r })
    }
}

//! The one JSON object every command prints on stdout (Bureau Design §6.2):
//!
//! ```json
//! {"ok": true, "data": {}, "warnings": [], "truncated": false}
//! {"ok": false, "error": {"code": "not_found", "message": "…", "retryable": false, "hint": "…"}}
//! ```
//!
//! Schema: `contracts/cli/envelope.schema.json`.

use crate::error::NewsError;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Envelope {
    Ok {
        ok: True,
        data: Value,
        warnings: Vec<String>,
        truncated: bool,
    },
    Err {
        ok: False,
        error: NewsError,
    },
}

/// `true`, as a type, so an `Ok` envelope cannot carry `"ok": false`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct True;
/// `false`, as a type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct False;

impl Serialize for True {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bool(true)
    }
}
impl Serialize for False {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bool(false)
    }
}

impl Envelope {
    pub fn ok(data: Value) -> Self {
        Envelope::Ok {
            ok: True,
            data,
            warnings: Vec::new(),
            truncated: false,
        }
    }

    pub fn ok_with(data: Value, warnings: Vec<String>, truncated: bool) -> Self {
        Envelope::Ok {
            ok: True,
            data,
            warnings,
            truncated,
        }
    }

    pub fn err(error: NewsError) -> Self {
        Envelope::Err { ok: False, error }
    }

    /// 0 for success, else the error code's exit code.
    pub fn exit_code(&self) -> i32 {
        match self {
            Envelope::Ok { .. } => 0,
            Envelope::Err { error, .. } => error.exit_code(),
        }
    }

    /// One compact line of JSON, without the trailing newline.
    pub fn to_line(&self) -> String {
        // Serialising plain data cannot fail; fall back to a fixed internal error rather than panic.
        serde_json::to_string(self).unwrap_or_else(|_| {
            r#"{"ok":false,"error":{"code":"internal","message":"envelope serialisation failed","retryable":false}}"#
                .to_owned()
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;
    use serde_json::json;

    #[test]
    fn ok_envelope_has_the_four_keys_in_order() {
        let line = Envelope::ok(json!({"a": 1})).to_line();
        assert_eq!(
            line,
            r#"{"ok":true,"data":{"a":1},"warnings":[],"truncated":false}"#
        );
    }

    #[test]
    fn err_envelope_carries_the_error_and_nothing_else() {
        let env = Envelope::err(NewsError::precondition("not built"));
        assert_eq!(env.exit_code(), 4);
        let v: Value = serde_json::from_str(&env.to_line()).unwrap();
        assert_eq!(v["ok"], false);
        assert_eq!(v["error"]["code"], "precondition");
        assert_eq!(v.as_object().unwrap().len(), 2);
    }

    #[test]
    fn exit_code_comes_from_the_error_code() {
        for code in ErrorCode::ALL {
            let env = Envelope::err(NewsError::new(code, "x"));
            assert_eq!(env.exit_code(), code.exit_code());
        }
        assert_eq!(Envelope::ok(Value::Null).exit_code(), 0);
    }

    #[test]
    fn line_is_single_line() {
        let env = Envelope::err(NewsError::usage("line one\nline two"));
        assert!(!env.to_line().contains('\n'));
    }
}

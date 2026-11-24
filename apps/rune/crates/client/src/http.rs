//! Troupe's HTTP routes: bearer auth, the `{ success, data }` body, and
//! `X-Command-Id` on ledgered mutations. [`parse_body`] is the contract; the
//! backends ([`crate::native::http_send`], [`crate::web::http_send`]) move
//! the bytes.

use serde::de::DeserializeOwned;
use serde_json::Value;

/// The header that carries a ledgered mutation's command id.
pub const COMMAND_ID_HEADER: &str = "X-Command-Id";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
}

impl Method {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
        }
    }
}

/// One HTTP request against the host.
#[derive(Clone, Debug, PartialEq)]
pub struct HttpRequest {
    pub method: Method,
    /// Starts with `/`.
    pub path: String,
    pub body: Option<Value>,
    pub command_id: Option<String>,
}

impl HttpRequest {
    #[must_use]
    pub fn get(path: &str) -> Self {
        Self {
            method: Method::Get,
            path: path.to_owned(),
            body: None,
            command_id: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HttpError {
    /// No answer, or an answer that is not a Troupe body.
    Network(String),
    /// The host refused with its own code (`UNAUTHORIZED`, `NOT_FOUND`, ...).
    Refused {
        status: u16,
        code: String,
        message: String,
    },
    /// A resend whose first run is still going.
    InProgress,
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(detail) => write!(f, "network error: {detail}"),
            Self::Refused {
                status,
                code,
                message,
            } => write!(f, "{code} (HTTP {status}): {message}"),
            Self::InProgress => f.write_str("the first run of this command is still going"),
        }
    }
}

impl std::error::Error for HttpError {}

/// Read a response body. A failure body keeps the host's code on any status;
/// a 2xx without a readable body is a network error. A unit reply has no
/// `data` and reads as `Value::Null`.
pub fn parse_body(status: u16, body: &[u8]) -> Result<Value, HttpError> {
    let ok = (200..300).contains(&status);
    let Ok(parsed) = serde_json::from_slice::<Value>(body) else {
        return Err(HttpError::Network(if ok {
            "invalid JSON body".into()
        } else {
            format!("HTTP {status}")
        }));
    };
    match parsed.get("success").and_then(Value::as_bool) {
        Some(false) => {
            let code = parsed.get("code").and_then(Value::as_str);
            let message = parsed.get("error").and_then(Value::as_str);
            match (code, message) {
                (Some(code), Some(message)) => Err(HttpError::Refused {
                    status,
                    code: code.to_owned(),
                    message: message.to_owned(),
                }),
                _ => Err(HttpError::Network(format!("HTTP {status}"))),
            }
        }
        Some(true) if ok => {
            if parsed.get("receipt").and_then(Value::as_str) == Some("in_progress") {
                return Err(HttpError::InProgress);
            }
            Ok(parsed.get("data").cloned().unwrap_or(Value::Null))
        }
        _ => Err(HttpError::Network(format!(
            "HTTP {status}: not a Troupe body"
        ))),
    }
}

/// Decode a route's `data` into its type.
pub fn decode<T: DeserializeOwned>(data: Value) -> Result<T, HttpError> {
    serde_json::from_value(data).map_err(|e| HttpError::Network(format!("unexpected body: {e}")))
}

/// One project of `GET /projects`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub color: String,
    /// `None` when the project has no working directory.
    pub path: Option<String>,
    #[serde(default)]
    pub is_archived: bool,
    #[serde(default)]
    pub is_default: bool,
}

/// `GET /projects` data.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectList {
    pub projects: Vec<ProjectSummary>,
    pub default_project_id: Option<String>,
}

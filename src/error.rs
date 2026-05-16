//! Error types for the SDK.

use std::fmt;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, SupabaseError>;

/// Top-level error type. Variants are routed by the originating Supabase service.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SupabaseError {
    #[error("PostgREST error: {0}")]
    Postgrest(#[from] PostgrestError),

    #[error("Auth error: {0}")]
    Auth(#[from] AuthError),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Transport error: {0}")]
    Transport(#[from] reqwest::Error),

    #[error("Decode error: {message}")]
    Decode { message: String, body: String },

    #[error("URL error: {0}")]
    Url(String),

    #[error("Invalid header: {0}")]
    InvalidHeader(String),

    #[error("Not found: {resource}")]
    NotFound { resource: String },

    #[error("Exceeded {attempts} retries (last status: {last_status:?})")]
    RetryExhausted { attempts: u32, last_status: Option<u16> },

    #[error("Serialization error: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("Unexpected error: {0}")]
    Unexpected(String),
}

impl From<url::ParseError> for SupabaseError {
    fn from(e: url::ParseError) -> Self {
        Self::Url(e.to_string())
    }
}

impl From<reqwest::header::InvalidHeaderValue> for SupabaseError {
    fn from(e: reqwest::header::InvalidHeaderValue) -> Self {
        Self::InvalidHeader(e.to_string())
    }
}

/// Structured PostgREST error body.
/// PostgREST returns `{"code": "...", "message": "...", "details": "...", "hint": "..."}`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PostgrestError {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub hint: Option<String>,
    #[serde(skip)]
    pub status: u16,
}

impl fmt::Display for PostgrestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.status, self.message)?;
        if let Some(code) = &self.code {
            write!(f, " (code: {code})")?;
        }
        if let Some(details) = &self.details {
            write!(f, " — {details}")?;
        }
        if let Some(hint) = &self.hint {
            write!(f, " [hint: {hint}]")?;
        }
        Ok(())
    }
}

impl std::error::Error for PostgrestError {}

/// Structured GoTrue (Auth) error body.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AuthError {
    #[serde(default)]
    pub code: Option<u32>,
    #[serde(default)]
    pub error_code: Option<String>,
    #[serde(default, alias = "msg", alias = "error_description", alias = "error")]
    pub message: String,
    #[serde(skip)]
    pub status: Option<u16>,
}

impl AuthError {
    pub fn from_message(msg: impl Into<String>) -> Self {
        Self {
            code: None,
            error_code: None,
            message: msg.into(),
            status: None,
        }
    }
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(status) = self.status {
            write!(f, "[{status}] ")?;
        }
        write!(f, "{}", self.message)?;
        if let Some(code) = &self.error_code {
            write!(f, " (error_code: {code})")?;
        }
        Ok(())
    }
}

impl std::error::Error for AuthError {}

/// Structured Storage API error body.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct StorageError {
    #[serde(default)]
    pub status_code: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub message: String,
    #[serde(skip)]
    pub status: Option<u16>,
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(status) = self.status {
            write!(f, "[{status}] ")?;
        }
        write!(f, "{}", self.message)?;
        if let Some(error) = &self.error {
            write!(f, " ({error})")?;
        }
        Ok(())
    }
}

impl std::error::Error for StorageError {}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use reqwest::StatusCode;
    use crate::universals::{decode_error, Service};

    // --- PostgrestError display ---

    #[test]
    fn postgrest_error_display_minimal() {
        let e = PostgrestError {
            code: None, message: "not found".into(), details: None, hint: None, status: 404,
        };
        let s = e.to_string();
        assert!(s.contains("404"), "display={s}");
        assert!(s.contains("not found"), "display={s}");
    }

    #[test]
    fn postgrest_error_display_full() {
        let e = PostgrestError {
            code: Some("23505".into()),
            message: "duplicate key".into(),
            details: Some("Key (id)=(1) already exists.".into()),
            hint: Some("Use upsert instead".into()),
            status: 409,
        };
        let s = e.to_string();
        assert!(s.contains("409"), "display={s}");
        assert!(s.contains("duplicate key"), "display={s}");
        assert!(s.contains("23505"), "display={s}");
        assert!(s.contains("Key (id)=(1)"), "display={s}");
        assert!(s.contains("Use upsert"), "display={s}");
    }

    #[test]
    fn postgrest_error_display_with_code_only() {
        let e = PostgrestError {
            code: Some("PGRST204".into()),
            message: "no rows".into(),
            details: None,
            hint: None,
            status: 204,
        };
        let s = e.to_string();
        assert!(s.contains("PGRST204"), "display={s}");
    }

    // --- AuthError display ---

    #[test]
    fn auth_error_display_with_status() {
        let e = AuthError {
            code: Some(401),
            error_code: Some("invalid_credentials".into()),
            message: "Invalid login".into(),
            status: Some(401),
        };
        let s = e.to_string();
        assert!(s.contains("401"), "display={s}");
        assert!(s.contains("Invalid login"), "display={s}");
        assert!(s.contains("invalid_credentials"), "display={s}");
    }

    #[test]
    fn auth_error_from_message() {
        let e = AuthError::from_message("something went wrong");
        assert_eq!(e.message, "something went wrong");
        assert!(e.code.is_none());
        assert!(e.status.is_none());
    }

    #[test]
    fn auth_error_display_without_status() {
        let e = AuthError::from_message("bad token");
        let s = e.to_string();
        assert_eq!(s, "bad token");
    }

    // --- StorageError display ---

    #[test]
    fn storage_error_display_with_error_field() {
        let e = StorageError {
            status_code: Some("404".into()),
            error: Some("Not Found".into()),
            message: "Object not found".into(),
            status: Some(404),
        };
        let s = e.to_string();
        assert!(s.contains("404"), "display={s}");
        assert!(s.contains("Object not found"), "display={s}");
        assert!(s.contains("Not Found"), "display={s}");
    }

    #[test]
    fn storage_error_display_minimal() {
        let e = StorageError {
            status_code: None, error: None, message: "forbidden".into(), status: Some(403),
        };
        let s = e.to_string();
        assert!(s.contains("403"), "display={s}");
        assert!(s.contains("forbidden"), "display={s}");
    }

    // --- SupabaseError display ---

    #[test]
    fn supabase_error_postgrest_variant() {
        let e = SupabaseError::Postgrest(PostgrestError {
            code: None, message: "oops".into(), details: None, hint: None, status: 500,
        });
        assert!(e.to_string().contains("PostgREST error"), "display={e}");
    }

    #[test]
    fn supabase_error_auth_variant() {
        let e = SupabaseError::Auth(AuthError::from_message("nope"));
        assert!(e.to_string().contains("Auth error"), "display={e}");
    }

    #[test]
    fn supabase_error_unexpected_variant() {
        let e = SupabaseError::Unexpected("mystery".into());
        assert!(e.to_string().contains("mystery"), "display={e}");
    }

    #[test]
    fn supabase_error_not_found_variant() {
        let e = SupabaseError::NotFound { resource: "users".into() };
        let s = e.to_string();
        assert!(s.contains("users"), "display={s}");
    }

    #[test]
    fn supabase_error_decode_variant() {
        let e = SupabaseError::Decode { message: "bad json".into(), body: "{}".into() };
        assert!(e.to_string().contains("Decode error"), "display={e}");
        assert!(e.to_string().contains("bad json"), "display={e}");
    }

    #[test]
    fn supabase_error_retry_exhausted() {
        let e = SupabaseError::RetryExhausted { attempts: 5, last_status: Some(429) };
        let s = e.to_string();
        assert!(s.contains("5"), "display={s}");
        assert!(s.contains("429"), "display={s}");
    }

    // --- decode_error routing ---

    #[test]
    fn decode_error_postgrest_parses_structured_body() {
        let body = r#"{"code":"23505","message":"duplicate key","details":"on id","hint":"use upsert"}"#;
        let e = decode_error(Service::Postgrest, StatusCode::CONFLICT, body);
        match e {
            SupabaseError::Postgrest(pe) => {
                assert_eq!(pe.code.as_deref(), Some("23505"));
                assert_eq!(pe.message, "duplicate key");
                assert_eq!(pe.status, 409);
            }
            other => panic!("expected Postgrest variant, got {other:?}"),
        }
    }

    #[test]
    fn decode_error_postgrest_fallback_on_plain_text() {
        let e = decode_error(Service::Postgrest, StatusCode::INTERNAL_SERVER_ERROR, "server exploded");
        match e {
            SupabaseError::Postgrest(pe) => {
                assert_eq!(pe.message, "server exploded");
                assert_eq!(pe.status, 500);
            }
            other => panic!("expected Postgrest, got {other:?}"),
        }
    }

    #[test]
    fn decode_error_postgrest_empty_body_uses_status_string() {
        let e = decode_error(Service::Postgrest, StatusCode::NOT_FOUND, "");
        match e {
            SupabaseError::Postgrest(pe) => {
                assert!(!pe.message.is_empty(), "message should not be empty");
                assert_eq!(pe.status, 404);
            }
            other => panic!("expected Postgrest, got {other:?}"),
        }
    }

    #[test]
    fn decode_error_auth_parses_gotrue_body() {
        let body = r#"{"error_code":"invalid_credentials","msg":"Invalid credentials","code":400}"#;
        let e = decode_error(Service::Auth, StatusCode::BAD_REQUEST, body);
        match e {
            SupabaseError::Auth(ae) => {
                assert_eq!(ae.error_code.as_deref(), Some("invalid_credentials"));
                assert_eq!(ae.status, Some(400));
            }
            other => panic!("expected Auth, got {other:?}"),
        }
    }

    #[test]
    fn decode_error_auth_fallback_plain_text() {
        let e = decode_error(Service::Auth, StatusCode::UNAUTHORIZED, "bad token");
        match e {
            SupabaseError::Auth(ae) => {
                assert_eq!(ae.message, "bad token");
                assert_eq!(ae.status, Some(401));
            }
            other => panic!("expected Auth, got {other:?}"),
        }
    }

    #[test]
    fn decode_error_storage_parses_body() {
        let body = r#"{"error":"Not Found","message":"Object missing","statusCode":"404"}"#;
        let e = decode_error(Service::Storage, StatusCode::NOT_FOUND, body);
        match e {
            SupabaseError::Storage(se) => {
                assert_eq!(se.message, "Object missing");
                assert_eq!(se.status, Some(404));
            }
            other => panic!("expected Storage, got {other:?}"),
        }
    }

    #[test]
    fn decode_error_functions_uses_storage_path() {
        // Functions errors are routed through the Storage error decoder.
        let e = decode_error(Service::Functions, StatusCode::INTERNAL_SERVER_ERROR, "fn crashed");
        match e {
            SupabaseError::Storage(se) => {
                assert_eq!(se.message, "fn crashed");
                assert_eq!(se.status, Some(500));
            }
            other => panic!("expected Storage, got {other:?}"),
        }
    }

    // --- From conversions ---

    #[test]
    fn from_url_parse_error() {
        let url_err = "::not a url::".parse::<url::Url>().unwrap_err();
        let e: SupabaseError = url_err.into();
        assert!(matches!(e, SupabaseError::Url(_)), "expected Url variant");
    }

    #[test]
    fn from_serde_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("not-json").unwrap_err();
        let e: SupabaseError = json_err.into();
        assert!(matches!(e, SupabaseError::Serialize(_)), "expected Serialize variant");
    }

    #[test]
    fn from_invalid_header_value() {
        // \r and \n are not valid in HeaderValue construction.
        let bad = reqwest::header::HeaderValue::from_str("bad\nvalue").unwrap_err();
        let e: SupabaseError = bad.into();
        match e {
            SupabaseError::InvalidHeader(msg) => assert!(!msg.is_empty()),
            other => panic!("expected InvalidHeader variant, got {other:?}"),
        }
    }

    #[test]
    fn storage_error_display_without_status_skips_prefix() {
        // The `status: None` branch — the `[NNN]` prefix should be absent.
        let e = StorageError {
            status_code: None,
            error: None,
            message: "no status here".into(),
            status: None,
        };
        let s = e.to_string();
        assert_eq!(s, "no status here");
    }

    #[test]
    fn auth_error_display_with_status_no_error_code() {
        // Cover the `Some(status)` branch *without* an error_code suffix.
        let e = AuthError {
            code: None,
            error_code: None,
            message: "boom".into(),
            status: Some(500),
        };
        let s = e.to_string();
        assert!(s.contains("500"), "display={s}");
        assert!(s.contains("boom"), "display={s}");
        assert!(!s.contains("error_code"), "display={s}");
    }

    #[test]
    fn supabase_error_storage_display() {
        let e = SupabaseError::Storage(StorageError {
            status_code: None,
            error: None,
            message: "x".into(),
            status: Some(500),
        });
        assert!(e.to_string().contains("Storage error"), "display={}", e);
    }

    #[test]
    fn supabase_error_url_display() {
        let e = SupabaseError::Url("not-a-url".into());
        assert!(e.to_string().contains("URL error"), "display={}", e);
    }

    #[test]
    fn supabase_error_invalid_header_display() {
        let e = SupabaseError::InvalidHeader("garbage".into());
        assert!(
            e.to_string().contains("Invalid header"),
            "display={}",
            e
        );
    }

    #[test]
    fn supabase_error_transport_display() {
        // Build a real reqwest error via an unreachable URL.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let client = reqwest::Client::new();
            let err = client
                .get("http://127.0.0.1:1") // refused
                .send()
                .await
                .unwrap_err();
            let se: SupabaseError = err.into();
            assert!(
                se.to_string().contains("Transport"),
                "display={se}"
            );
        });
    }

    #[test]
    fn supabase_error_serialize_display() {
        let json_err = serde_json::from_str::<serde_json::Value>("not-json").unwrap_err();
        let e: SupabaseError = json_err.into();
        assert!(e.to_string().contains("Serialization"), "display={}", e);
    }
}

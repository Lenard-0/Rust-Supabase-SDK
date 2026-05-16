use reqwest::{header::HeaderMap, Method, RequestBuilder, Response, StatusCode};
use serde_json::Value;
use tracing::{debug, warn};

use crate::error::{AuthError, PostgrestError, Result, StorageError, SupabaseError};
use crate::SupabaseClient;

/// Which Supabase service the request is targeting. Determines how non-2xx responses are decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    Postgrest,
    Auth,
    Storage,
    Functions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl HttpMethod {
    pub fn as_reqwest(&self) -> Method {
        match self {
            Self::Get => Method::GET,
            Self::Post => Method::POST,
            Self::Put => Method::PUT,
            Self::Patch => Method::PATCH,
            Self::Delete => Method::DELETE,
        }
    }
}

/// Per-request options that don't fit cleanly into the path/method/payload triple.
#[derive(Debug, Default, Clone)]
pub struct RequestOptions {
    /// Service classification for error decoding. Defaults to `Postgrest`.
    pub service: Option<Service>,
    /// Adds `Prefer: resolution=merge-duplicates` (Postgres upsert behavior).
    pub upsert: bool,
    /// Extra `Prefer:` directives, e.g. `return=representation`, `count=exact`.
    pub prefer: Vec<String>,
    /// Additional headers merged on top of the client defaults.
    pub headers: Vec<(String, String)>,
    /// Override the client's default access token / use a request-scoped one (e.g. user JWT).
    pub bearer_override: Option<String>,
}

impl RequestOptions {
    pub fn postgrest() -> Self {
        Self { service: Some(Service::Postgrest), ..Self::default() }
    }
    pub fn auth() -> Self {
        Self { service: Some(Service::Auth), ..Self::default() }
    }
    pub fn storage() -> Self {
        Self { service: Some(Service::Storage), ..Self::default() }
    }
}

impl SupabaseClient {
    /// Build a `reqwest::RequestBuilder` with the auth headers, schema headers, and
    /// global headers applied. Callers can attach a body or extra headers as needed.
    pub fn build_request(&self, method: Method, url: &str, opts: &RequestOptions) -> RequestBuilder {
        let bearer = match opts.bearer_override.clone() {
            Some(b) => b,
            None => self.effective_bearer(),
        };

        let mut req = self
            .http
            .request(method, url)
            .header("apikey", &self.api_key)
            .bearer_auth(bearer);

        // Schema header (for PostgREST `schema()` selection).
        if let Some(schema) = &self.schema {
            req = req
                .header("Accept-Profile", schema)
                .header("Content-Profile", schema);
        }

        // Client-wide extra headers.
        for (k, v) in &self.extra_headers {
            req = req.header(k.as_str(), v);
        }

        // Per-request extra headers.
        for (k, v) in &opts.headers {
            req = req.header(k.as_str(), v);
        }

        // Prefer header (aggregated).
        let mut prefer: Vec<String> = opts.prefer.clone();
        if opts.upsert {
            prefer.push("resolution=merge-duplicates".to_string());
        }
        if !prefer.is_empty() {
            req = req.header("Prefer", prefer.join(","));
        }

        req
    }

    /// Generic request helper used by every module. Handles 429 retries, structured
    /// error decoding per service, and empty-body responses.
    pub async fn request(
        &self,
        path: &str,
        method: HttpMethod,
        payload: Option<Value>,
        upsert: bool,
    ) -> Result<Value> {
        let opts = RequestOptions {
            upsert,
            ..RequestOptions::postgrest()
        };
        self.request_with(path, method, payload, &opts).await
    }

    /// Like `request`, but takes a fully-specified `RequestOptions`.
    pub async fn request_with(
        &self,
        path: &str,
        method: HttpMethod,
        payload: Option<Value>,
        opts: &RequestOptions,
    ) -> Result<Value> {
        let resp = self.send(path, method, payload, opts).await?;
        let (status, _headers, body) = read_response(resp).await?;

        if !status.is_success() {
            return Err(decode_error(
                opts.service.unwrap_or(Service::Postgrest),
                status,
                &body,
            ));
        }

        if body.is_empty() {
            return Ok(Value::Null);
        }

        serde_json::from_str(&body).map_err(|e| SupabaseError::Decode {
            message: e.to_string(),
            body,
        })
    }

    /// Like `request_with`, but for raw binary bodies (Storage uploads).
    /// Sends the `body` bytes with the supplied `Content-Type` and applies
    /// the same auth + Prefer headers as `request_with`.
    pub async fn request_bytes(
        &self,
        path: &str,
        method: HttpMethod,
        body: Vec<u8>,
        content_type: &str,
        opts: &RequestOptions,
    ) -> Result<Value> {
        let max_retries: u32 = self.retry.max_retries;
        let base_backoff = self.retry.base_backoff;
        let url = format!("{}{}", self.url, path);

        let mut last_status: Option<u16> = None;
        for attempt in 0..=max_retries {
            let req = self
                .build_request(method.as_reqwest(), &url, opts)
                .header("Content-Type", content_type)
                .body(body.clone());

            let resp = req.send().await?;
            let status = resp.status();

            if status == StatusCode::TOO_MANY_REQUESTS && attempt < max_retries {
                last_status = Some(status.as_u16());
                let backoff = base_backoff
                    .checked_mul(2_u32.saturating_pow(attempt))
                    .unwrap_or(base_backoff);
                warn!(target: "supabase", attempt, ?backoff, "429 Too Many Requests; backing off");
                tokio::time::sleep(backoff).await;
                continue;
            }

            let (status, _headers, body_text) = read_response(resp).await?;
            if !status.is_success() {
                return Err(decode_error(
                    opts.service.unwrap_or(Service::Storage),
                    status,
                    &body_text,
                ));
            }
            if body_text.is_empty() {
                return Ok(Value::Null);
            }
            return serde_json::from_str(&body_text).map_err(|e| SupabaseError::Decode {
                message: e.to_string(),
                body: body_text,
            });
        }

        Err(SupabaseError::RetryExhausted {
            attempts: max_retries,
            last_status,
        })
    }

    /// Send a request and return the raw [`reqwest::Response`] for streaming
    /// or large-body consumption. Errors decode through the service-aware path.
    pub async fn request_streaming(
        &self,
        path: &str,
        method: HttpMethod,
        opts: &RequestOptions,
    ) -> Result<Response> {
        let resp = self.send(path, method, None, opts).await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await?;
            return Err(decode_error(
                opts.service.unwrap_or(Service::Storage),
                status,
                &body,
            ));
        }
        Ok(resp)
    }

    /// Like `request_with`, but also returns the response headers (needed by `count`).
    pub async fn request_full(
        &self,
        path: &str,
        method: HttpMethod,
        payload: Option<Value>,
        opts: &RequestOptions,
    ) -> Result<(StatusCode, HeaderMap, String)> {
        let resp = self.send(path, method, payload, opts).await?;
        let (status, headers, body) = read_response(resp).await?;

        if !status.is_success() {
            return Err(decode_error(
                opts.service.unwrap_or(Service::Postgrest),
                status,
                &body,
            ));
        }

        Ok((status, headers, body))
    }

    async fn send(
        &self,
        path: &str,
        method: HttpMethod,
        payload: Option<Value>,
        opts: &RequestOptions,
    ) -> Result<Response> {
        let max_retries: u32 = self.retry.max_retries;
        let base_backoff = self.retry.base_backoff;
        let url = format!("{}{}", self.url, path);
        debug!(target: "supabase", %url, ?method, service = ?opts.service, "sending request");

        let mut last_status: Option<u16> = None;
        for attempt in 0..=max_retries {
            let mut req = self.build_request(method.as_reqwest(), &url, opts);
            if let Some(body) = &payload {
                req = req.json(body);
            }

            let resp = req.send().await?;
            let status = resp.status();
            debug!(target: "supabase", %url, status = status.as_u16(), attempt, "received response");

            if status == StatusCode::TOO_MANY_REQUESTS && attempt < max_retries {
                last_status = Some(status.as_u16());
                let backoff = base_backoff
                    .checked_mul(2_u32.saturating_pow(attempt))
                    .unwrap_or(base_backoff);
                warn!(target: "supabase", attempt, ?backoff, "429 Too Many Requests; backing off");
                tokio::time::sleep(backoff).await;
                continue;
            }

            return Ok(resp);
        }

        Err(SupabaseError::RetryExhausted {
            attempts: max_retries,
            last_status,
        })
    }
}

async fn read_response(resp: Response) -> Result<(StatusCode, HeaderMap, String)> {
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = resp.text().await?;
    Ok((status, headers, body))
}

/// Decode a non-2xx response body into the appropriate structured error variant.
pub(crate) fn decode_error(service: Service, status: StatusCode, body: &str) -> SupabaseError {
    let status_code = status.as_u16();
    match service {
        Service::Postgrest => {
            if let Ok(mut e) = serde_json::from_str::<PostgrestError>(body) {
                e.status = status_code;
                return SupabaseError::Postgrest(e);
            }
            SupabaseError::Postgrest(PostgrestError {
                code: None,
                message: if body.is_empty() { status.to_string() } else { body.to_string() },
                details: None,
                hint: None,
                status: status_code,
            })
        }
        Service::Auth => {
            if let Ok(mut e) = serde_json::from_str::<AuthError>(body) {
                e.status = Some(status_code);
                return SupabaseError::Auth(e);
            }
            SupabaseError::Auth(AuthError {
                code: None,
                error_code: None,
                message: if body.is_empty() { status.to_string() } else { body.to_string() },
                status: Some(status_code),
            })
        }
        Service::Storage | Service::Functions => {
            if let Ok(mut e) = serde_json::from_str::<StorageError>(body) {
                e.status = Some(status_code);
                return SupabaseError::Storage(e);
            }
            SupabaseError::Storage(StorageError {
                status_code: Some(status_code.to_string()),
                error: None,
                message: if body.is_empty() { status.to_string() } else { body.to_string() },
                status: Some(status_code),
            })
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // --- HttpMethod ---

    #[test]
    fn http_method_as_reqwest_all_variants() {
        assert_eq!(HttpMethod::Get.as_reqwest(), reqwest::Method::GET);
        assert_eq!(HttpMethod::Post.as_reqwest(), reqwest::Method::POST);
        assert_eq!(HttpMethod::Put.as_reqwest(), reqwest::Method::PUT);
        assert_eq!(HttpMethod::Patch.as_reqwest(), reqwest::Method::PATCH);
        assert_eq!(HttpMethod::Delete.as_reqwest(), reqwest::Method::DELETE);
    }

    #[test]
    fn http_method_eq_and_copy() {
        let m = HttpMethod::Post;
        assert_eq!(m, m);
        assert_ne!(HttpMethod::Get, HttpMethod::Post);
    }

    // --- Service ---

    #[test]
    fn service_eq() {
        assert_eq!(Service::Postgrest, Service::Postgrest);
        assert_ne!(Service::Auth, Service::Storage);
        assert_ne!(Service::Functions, Service::Postgrest);
    }

    // --- RequestOptions constructors ---

    #[test]
    fn request_options_postgrest() {
        let opts = RequestOptions::postgrest();
        assert_eq!(opts.service, Some(Service::Postgrest));
        assert!(!opts.upsert);
        assert!(opts.prefer.is_empty());
        assert!(opts.headers.is_empty());
        assert!(opts.bearer_override.is_none());
    }

    #[test]
    fn request_options_auth() {
        let opts = RequestOptions::auth();
        assert_eq!(opts.service, Some(Service::Auth));
    }

    #[test]
    fn request_options_storage() {
        let opts = RequestOptions::storage();
        assert_eq!(opts.service, Some(Service::Storage));
    }

    #[test]
    fn request_options_default_has_no_service() {
        let opts = RequestOptions::default();
        assert!(opts.service.is_none());
    }
}

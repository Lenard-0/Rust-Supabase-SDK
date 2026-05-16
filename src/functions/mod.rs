//! Supabase Edge Functions — invoke a deployed function by name.
//!
//! Mirrors `supabase-js`'s `client.functions.invoke(name, options)`:
//!
//! ```no_run
//! # use rust_supabase_sdk::SupabaseClient;
//! # use serde::{Serialize, Deserialize};
//! #[derive(Serialize)]
//! struct Req<'a> { name: &'a str }
//! #[derive(Deserialize, Debug)]
//! struct Res { greeting: String }
//!
//! # async fn demo(client: SupabaseClient) -> rust_supabase_sdk::Result<()> {
//! let res: Res = client
//!     .functions()
//!     .invoke("hello", &Req { name: "world" })
//!     .await?;
//! println!("{}", res.greeting);
//! # Ok(()) }
//! ```
//!
//! Use [`Functions::invoke_with`] to attach custom headers, switch HTTP method,
//! route to a specific region, or send non-JSON bodies. Use
//! [`Functions::invoke_stream`] to receive a raw [`reqwest::Response`] for
//! streaming responses (e.g. server-sent events from an Edge Function).

use serde::{de::DeserializeOwned, Serialize};

use crate::error::{Result, SupabaseError};
use crate::universals::{HttpMethod, RequestOptions, Service};
use crate::SupabaseClient;

impl SupabaseClient {
    /// Open the Edge Functions namespace.
    pub fn functions(&self) -> Functions {
        Functions { client: self.clone() }
    }
}

/// The `functions` namespace.
#[derive(Debug, Clone)]
pub struct Functions {
    pub(crate) client: SupabaseClient,
}

/// HTTP method to use when invoking an Edge Function. Defaults to `POST`,
/// matching `supabase-js`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum InvokeMethod {
    Get,
    #[default]
    Post,
    Put,
    Patch,
    Delete,
}

impl InvokeMethod {
    fn to_http(self) -> HttpMethod {
        match self {
            Self::Get => HttpMethod::Get,
            Self::Post => HttpMethod::Post,
            Self::Put => HttpMethod::Put,
            Self::Patch => HttpMethod::Patch,
            Self::Delete => HttpMethod::Delete,
        }
    }
}

/// Region hint sent via the `x-region` header. Matches the canonical names
/// accepted by Supabase Edge Functions; use [`FunctionRegion::Custom`] for
/// any value not enumerated here.
#[derive(Debug, Clone)]
pub enum FunctionRegion {
    Any,
    ApNortheast1,
    ApNortheast2,
    ApSouth1,
    ApSoutheast1,
    ApSoutheast2,
    CaCentral1,
    EuCentral1,
    EuWest1,
    EuWest2,
    EuWest3,
    SaEast1,
    UsEast1,
    UsWest1,
    UsWest2,
    Custom(String),
}

impl FunctionRegion {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Any => "any",
            Self::ApNortheast1 => "ap-northeast-1",
            Self::ApNortheast2 => "ap-northeast-2",
            Self::ApSouth1 => "ap-south-1",
            Self::ApSoutheast1 => "ap-southeast-1",
            Self::ApSoutheast2 => "ap-southeast-2",
            Self::CaCentral1 => "ca-central-1",
            Self::EuCentral1 => "eu-central-1",
            Self::EuWest1 => "eu-west-1",
            Self::EuWest2 => "eu-west-2",
            Self::EuWest3 => "eu-west-3",
            Self::SaEast1 => "sa-east-1",
            Self::UsEast1 => "us-east-1",
            Self::UsWest1 => "us-west-1",
            Self::UsWest2 => "us-west-2",
            Self::Custom(s) => s.as_str(),
        }
    }
}

/// Body payload for an invocation. Matches the JS SDK's auto-detection: pick
/// the variant that best fits your data, and the SDK sets the right
/// `Content-Type` for you.
#[derive(Debug, Default, Clone)]
pub enum InvokeBody {
    #[default]
    Empty,
    /// JSON-serialized body. `Content-Type: application/json`.
    Json(serde_json::Value),
    /// Raw bytes with an explicit `Content-Type` header.
    Bytes { data: Vec<u8>, content_type: String },
    /// `Content-Type: text/plain`.
    Text(String),
    /// `application/x-www-form-urlencoded`.
    Form(Vec<(String, String)>),
}

/// Options for [`Functions::invoke_with`] / [`Functions::invoke_stream`].
///
/// Build with `Default::default()` and chain setters, mirroring supabase-js's
/// `{ body, headers, region, method }` options object.
#[derive(Debug, Default, Clone)]
pub struct InvokeOptions {
    pub body: InvokeBody,
    pub headers: Vec<(String, String)>,
    pub region: Option<FunctionRegion>,
    pub method: Option<InvokeMethod>,
}

impl InvokeOptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach a JSON body. Returns an error if `body` fails to serialize.
    pub fn body_json<T: Serialize + ?Sized>(mut self, body: &T) -> Result<Self> {
        self.body = InvokeBody::Json(serde_json::to_value(body)?);
        Ok(self)
    }

    pub fn body_bytes(mut self, data: Vec<u8>, content_type: impl Into<String>) -> Self {
        self.body = InvokeBody::Bytes { data, content_type: content_type.into() };
        self
    }

    pub fn body_text(mut self, text: impl Into<String>) -> Self {
        self.body = InvokeBody::Text(text.into());
        self
    }

    pub fn body_form(mut self, fields: Vec<(String, String)>) -> Self {
        self.body = InvokeBody::Form(fields);
        self
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn region(mut self, region: FunctionRegion) -> Self {
        self.region = Some(region);
        self
    }

    pub fn method(mut self, method: InvokeMethod) -> Self {
        self.method = Some(method);
        self
    }
}

impl Functions {
    fn endpoint(&self, name: &str) -> String {
        // `name` may include sub-paths like `"hello/world"`; keep as-is.
        format!("/functions/v1/{name}")
    }

    /// Invoke a function with a JSON body and decode the JSON response.
    /// This is the supabase-js fast path. The user's access token is attached
    /// automatically when the session store has one.
    pub async fn invoke<Req, Res>(&self, name: &str, body: &Req) -> Result<Res>
    where
        Req: Serialize + ?Sized,
        Res: DeserializeOwned,
    {
        let opts = InvokeOptions::new().body_json(body)?;
        self.invoke_with(name, opts).await
    }

    /// Invoke a function with the full option set. Decodes the response as
    /// JSON. For non-JSON responses, use [`invoke_stream`](Self::invoke_stream).
    pub async fn invoke_with<Res>(&self, name: &str, options: InvokeOptions) -> Result<Res>
    where
        Res: DeserializeOwned,
    {
        let resp = self.send(name, options).await?;
        let status = resp.status();
        let bytes = resp
            .bytes()
            .await
            .map_err(SupabaseError::Transport)?;

        if !status.is_success() {
            let body = String::from_utf8_lossy(&bytes).into_owned();
            return Err(crate::universals::decode_error(
                Service::Functions,
                status,
                &body,
            ));
        }

        if bytes.is_empty() {
            // Synthesize a JSON null so callers asking for `Option<T>` or `()` succeed.
            return serde_json::from_str("null").map_err(|e| SupabaseError::Decode {
                message: e.to_string(),
                body: String::new(),
            });
        }

        serde_json::from_slice(&bytes).map_err(|e| SupabaseError::Decode {
            message: e.to_string(),
            body: String::from_utf8_lossy(&bytes).into_owned(),
        })
    }

    /// Invoke a function and return the raw [`reqwest::Response`]. Use this
    /// to stream the body via `Response::bytes_stream` (requires the `stream`
    /// feature on `reqwest`) or to inspect headers directly.
    pub async fn invoke_stream(
        &self,
        name: &str,
        options: InvokeOptions,
    ) -> Result<reqwest::Response> {
        let resp = self.send(name, options).await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(crate::universals::decode_error(
                Service::Functions,
                status,
                &body,
            ));
        }
        Ok(resp)
    }

    async fn send(&self, name: &str, options: InvokeOptions) -> Result<reqwest::Response> {
        let path = self.endpoint(name);
        let url = format!("{}{}", self.client.url, path);
        let method = options.method.unwrap_or_default().to_http();

        let mut req_opts = RequestOptions {
            service: Some(Service::Functions),
            ..RequestOptions::default()
        };
        req_opts.headers.extend(options.headers.iter().cloned());
        if let Some(region) = &options.region {
            req_opts
                .headers
                .push(("x-region".into(), region.as_str().to_string()));
        }

        let req = self
            .client
            .build_request(method.as_reqwest(), &url, &req_opts);

        let req = match options.body {
            InvokeBody::Empty => req,
            InvokeBody::Json(v) => req.header("Content-Type", "application/json").json(&v),
            InvokeBody::Bytes { data, content_type } => {
                req.header("Content-Type", content_type).body(data)
            }
            InvokeBody::Text(t) => req.header("Content-Type", "text/plain").body(t),
            InvokeBody::Form(fields) => req
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(serde_urlencoded_form(&fields)),
        };

        req.send().await.map_err(SupabaseError::Transport)
    }
}

/// Minimal `application/x-www-form-urlencoded` encoder so we don't pull in
/// `serde_urlencoded` as a new dep. Pairs are joined with `&`, each `k=v`
/// percent-encoded with the standard form-encoding rules (spaces -> `+`).
fn serde_urlencoded_form(fields: &[(String, String)]) -> String {
    let mut out = String::new();
    for (i, (k, v)) in fields.iter().enumerate() {
        if i > 0 {
            out.push('&');
        }
        out.push_str(&form_encode(k));
        out.push('=');
        out.push_str(&form_encode(v));
    }
    out
}

fn form_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn region_strings() {
        assert_eq!(FunctionRegion::UsEast1.as_str(), "us-east-1");
        assert_eq!(FunctionRegion::Any.as_str(), "any");
        assert_eq!(
            FunctionRegion::Custom("af-south-1".into()).as_str(),
            "af-south-1"
        );
    }

    #[test]
    fn endpoint_path() {
        let client = SupabaseClient::new("https://example.supabase.co", "k", None);
        let f = client.functions();
        assert_eq!(f.endpoint("hello"), "/functions/v1/hello");
        assert_eq!(f.endpoint("greet/world"), "/functions/v1/greet/world");
    }

    #[test]
    fn invoke_method_default_is_post() {
        assert_eq!(InvokeMethod::default(), InvokeMethod::Post);
    }

    #[test]
    fn options_body_json_serializes() {
        #[derive(Serialize)]
        struct Req<'a> {
            name: &'a str,
        }
        let opts = InvokeOptions::new().body_json(&Req { name: "x" }).unwrap();
        match opts.body {
            InvokeBody::Json(v) => assert_eq!(v["name"], "x"),
            _ => panic!("expected Json body"),
        }
    }

    #[test]
    fn options_chain() {
        let opts = InvokeOptions::new()
            .body_text("ping")
            .header("X-Custom", "v")
            .region(FunctionRegion::EuWest1)
            .method(InvokeMethod::Put);
        assert!(matches!(opts.body, InvokeBody::Text(ref s) if s == "ping"));
        assert_eq!(opts.headers, vec![("X-Custom".into(), "v".into())]);
        assert!(matches!(opts.region, Some(FunctionRegion::EuWest1)));
        assert_eq!(opts.method, Some(InvokeMethod::Put));
    }

    #[test]
    fn form_encode_spaces_and_specials() {
        assert_eq!(form_encode("a b"), "a+b");
        assert_eq!(form_encode("hello world!"), "hello+world%21");
        assert_eq!(form_encode("a&b=c"), "a%26b%3Dc");
    }

    #[test]
    fn form_body_renders_kv_pairs() {
        let s = serde_urlencoded_form(&[
            ("name".into(), "alice".into()),
            ("msg".into(), "hi there".into()),
        ]);
        assert_eq!(s, "name=alice&msg=hi+there");
    }

    // --- FunctionRegion — all variants ---

    #[test]
    fn region_all_named_variants() {
        let cases = [
            (FunctionRegion::Any, "any"),
            (FunctionRegion::ApNortheast1, "ap-northeast-1"),
            (FunctionRegion::ApNortheast2, "ap-northeast-2"),
            (FunctionRegion::ApSouth1, "ap-south-1"),
            (FunctionRegion::ApSoutheast1, "ap-southeast-1"),
            (FunctionRegion::ApSoutheast2, "ap-southeast-2"),
            (FunctionRegion::CaCentral1, "ca-central-1"),
            (FunctionRegion::EuCentral1, "eu-central-1"),
            (FunctionRegion::EuWest1, "eu-west-1"),
            (FunctionRegion::EuWest2, "eu-west-2"),
            (FunctionRegion::EuWest3, "eu-west-3"),
            (FunctionRegion::SaEast1, "sa-east-1"),
            (FunctionRegion::UsEast1, "us-east-1"),
            (FunctionRegion::UsWest1, "us-west-1"),
            (FunctionRegion::UsWest2, "us-west-2"),
        ];
        for (region, expected) in cases {
            assert_eq!(region.as_str(), expected, "region mismatch for {expected}");
        }
    }

    #[test]
    fn region_custom_returns_inner_string() {
        let r = FunctionRegion::Custom("me-south-1".into());
        assert_eq!(r.as_str(), "me-south-1");
    }

    // --- InvokeMethod ---

    #[test]
    fn invoke_method_all_to_http() {
        assert!(matches!(InvokeMethod::Get.to_http(), HttpMethod::Get));
        assert!(matches!(InvokeMethod::Post.to_http(), HttpMethod::Post));
        assert!(matches!(InvokeMethod::Put.to_http(), HttpMethod::Put));
        assert!(matches!(InvokeMethod::Patch.to_http(), HttpMethod::Patch));
        assert!(matches!(InvokeMethod::Delete.to_http(), HttpMethod::Delete));
    }

    #[test]
    fn invoke_method_clone_and_eq() {
        let m = InvokeMethod::Patch;
        assert_eq!(m, m.clone());
        assert_ne!(InvokeMethod::Get, InvokeMethod::Post);
    }

    // --- InvokeBody ---

    #[test]
    fn invoke_body_default_is_empty() {
        assert!(matches!(InvokeBody::default(), InvokeBody::Empty));
    }

    #[test]
    fn invoke_body_clone_preserves_variant() {
        let b = InvokeBody::Text("hello".into());
        let b2 = b.clone();
        assert!(matches!(b2, InvokeBody::Text(ref s) if s == "hello"));
    }

    #[test]
    fn invoke_body_bytes_round_trip() {
        let data = b"binary data".to_vec();
        let b = InvokeBody::Bytes { data: data.clone(), content_type: "application/octet-stream".into() };
        match b {
            InvokeBody::Bytes { data: d, content_type: ct } => {
                assert_eq!(d, data);
                assert_eq!(ct, "application/octet-stream");
            }
            _ => panic!("expected Bytes"),
        }
    }

    #[test]
    fn invoke_body_form_holds_pairs() {
        let pairs = vec![("a".into(), "1".into()), ("b".into(), "2".into())];
        let b = InvokeBody::Form(pairs.clone());
        match b {
            InvokeBody::Form(p) => assert_eq!(p, pairs),
            _ => panic!("expected Form"),
        }
    }

    // --- InvokeOptions builder ---

    #[test]
    fn options_default_has_empty_body_and_no_region() {
        let opts = InvokeOptions::default();
        assert!(matches!(opts.body, InvokeBody::Empty));
        assert!(opts.headers.is_empty());
        assert!(opts.region.is_none());
        assert!(opts.method.is_none());
    }

    #[test]
    fn options_body_bytes_sets_variant() {
        let opts = InvokeOptions::new().body_bytes(vec![1, 2, 3], "image/png");
        assert!(matches!(opts.body, InvokeBody::Bytes { .. }));
    }

    #[test]
    fn options_body_form_sets_variant() {
        let opts = InvokeOptions::new()
            .body_form(vec![("key".into(), "val".into())]);
        assert!(matches!(opts.body, InvokeBody::Form(_)));
    }

    #[test]
    fn options_multiple_headers_accumulate() {
        let opts = InvokeOptions::new()
            .header("X-A", "1")
            .header("X-B", "2")
            .header("X-C", "3");
        assert_eq!(opts.headers.len(), 3);
        assert_eq!(opts.headers[1], ("X-B".into(), "2".into()));
    }

    #[test]
    fn options_region_and_method_round_trip() {
        let opts = InvokeOptions::new()
            .region(FunctionRegion::UsWest2)
            .method(InvokeMethod::Delete);
        assert!(matches!(opts.region, Some(FunctionRegion::UsWest2)));
        assert_eq!(opts.method, Some(InvokeMethod::Delete));
    }

    #[test]
    fn options_body_json_with_nested_value() {
        let v = serde_json::json!({ "user": { "id": 42, "active": true } });
        let opts = InvokeOptions::new().body_json(&v).unwrap();
        match &opts.body {
            InvokeBody::Json(j) => assert_eq!(j["user"]["id"], 42),
            _ => panic!("expected Json body"),
        }
    }

    // --- form_encode edge cases ---

    #[test]
    fn form_encode_empty_string() {
        assert_eq!(form_encode(""), "");
    }

    #[test]
    fn form_encode_alphanumeric_passthrough() {
        assert_eq!(form_encode("abc123"), "abc123");
        assert_eq!(form_encode("ABC"), "ABC");
    }

    #[test]
    fn form_encode_unreserved_chars_passthrough() {
        // RFC 3986 unreserved: '-', '.', '_', '~'
        assert_eq!(form_encode("foo-bar.baz_qux~"), "foo-bar.baz_qux~");
    }

    #[test]
    fn form_encode_slash_encoded() {
        assert_eq!(form_encode("/path/segment"), "%2Fpath%2Fsegment");
    }

    #[test]
    fn form_encode_percent_encoded() {
        assert_eq!(form_encode("100%"), "100%25");
    }

    #[test]
    fn form_encode_multiple_spaces() {
        assert_eq!(form_encode("hello world foo"), "hello+world+foo");
    }

    // --- serde_urlencoded_form edge cases ---

    #[test]
    fn form_body_empty_pairs() {
        assert_eq!(serde_urlencoded_form(&[]), "");
    }

    #[test]
    fn form_body_single_pair() {
        assert_eq!(serde_urlencoded_form(&[("key".into(), "value".into())]), "key=value");
    }

    #[test]
    fn form_body_special_chars_in_key_and_value() {
        let s = serde_urlencoded_form(&[("a[0]".into(), "val ue".into())]);
        assert_eq!(s, "a%5B0%5D=val+ue");
    }

    // --- endpoint ---

    #[test]
    fn endpoint_with_sub_path() {
        let client = SupabaseClient::new("https://proj.supabase.co", "key", None);
        let f = client.functions();
        assert_eq!(f.endpoint("v2/greet/world"), "/functions/v1/v2/greet/world");
    }

    #[test]
    fn endpoint_empty_name() {
        let client = SupabaseClient::new("https://proj.supabase.co", "key", None);
        let f = client.functions();
        assert_eq!(f.endpoint(""), "/functions/v1/");
    }
}

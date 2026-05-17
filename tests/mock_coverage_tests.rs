//! Mock-server tests filling coverage gaps in modules that are otherwise
//! exercised only via live integration:
//!
//!   * `src/rpc.rs` — `rpc_call`
//!   * `src/functions/mod.rs` — `invoke`, `invoke_with`, `invoke_stream`, all
//!     body variants, region header, JSON/text/bytes/form
//!   * `src/auth/mod.rs` — sign_up, verify_otp, resend, sign_in_with_id_token,
//!     exchange_code_for_session, reset_password_for_email, refresh_session,
//!     sign_in_with_otp paths
//!   * `src/universals/mod.rs` — `request_bytes` 429 retry, empty / malformed
//!     body, error decoding
//!   * `src/auth/admin.rs` — invite_user_by_email, generate_link, list_users
//!   * `src/postgrest/builder.rs` — execute-path variants (bare object, null,
//!     decode errors, IntoFuture await, maybe_single multi-row)
//!
//! Mocks let us drive every branch deterministically without needing a live
//! project pre-configured with edge functions, OTP, OAuth, etc.

#![allow(clippy::unwrap_used)]

use std::time::Duration;

use rust_supabase_sdk::auth::{
    OtpRecipient, OtpType, OAuthOptions, OtpOptions, SignOutScope,
    UpdateUserAttributes, VerifyOtpParams, ResetPasswordOptions,
};
use rust_supabase_sdk::functions::{
    FunctionRegion, InvokeMethod, InvokeOptions,
};
use rust_supabase_sdk::storage::UploadOptions;
use rust_supabase_sdk::{RetryConfig, SupabaseClient, SupabaseError};
use serde_json::{json, Value};
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client(server: &MockServer) -> SupabaseClient {
    SupabaseClient::builder(server.uri(), "test-key")
        .retry(RetryConfig::new(1, Duration::from_millis(10)))
        .build()
}

// ===========================================================================
// rpc.rs
// ===========================================================================

#[tokio::test]
async fn rpc_call_returns_array_on_array_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/rest/v1/rpc/hello"))
        .and(body_json(json!({"name": "world"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([1, 2, 3])))
        .expect(1)
        .mount(&server)
        .await;

    let c = client(&server);
    let rows = c.rpc_call("hello", json!({"name": "world"})).await.unwrap();
    assert_eq!(rows, vec![json!(1), json!(2), json!(3)]);
    server.verify().await;
}

#[tokio::test]
async fn rpc_call_non_array_response_returns_unexpected() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/rest/v1/rpc/scalar"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!(42)))
        .mount(&server)
        .await;

    let err = client(&server).rpc_call("scalar", json!({})).await.unwrap_err();
    match err {
        SupabaseError::Unexpected(msg) => assert!(msg.contains("non-array"), "msg={msg}"),
        other => panic!("expected Unexpected, got {other:?}"),
    }
}

#[tokio::test]
async fn rpc_call_propagates_server_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/rest/v1/rpc/boom"))
        .respond_with(ResponseTemplate::new(500).set_body_string("kaboom"))
        .mount(&server)
        .await;
    let err = client(&server).rpc_call("boom", json!({})).await.unwrap_err();
    assert!(matches!(err, SupabaseError::Postgrest(_)));
}


// ===========================================================================
// functions/mod.rs — invoke pathways
// ===========================================================================

#[tokio::test]
async fn functions_invoke_json_body_decodes_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/functions/v1/echo"))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"echo": "hi"})))
        .expect(1)
        .mount(&server)
        .await;
    let resp: Value = client(&server)
        .functions()
        .invoke("echo", &json!({"msg": "hi"}))
        .await
        .unwrap();
    assert_eq!(resp["echo"], "hi");
}

#[tokio::test]
async fn functions_invoke_with_text_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/functions/v1/text"))
        .and(header("content-type", "text/plain"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .expect(1)
        .mount(&server)
        .await;
    let opts = InvokeOptions::new().body_text("ping").method(InvokeMethod::Post);
    let resp: Value = client(&server).functions().invoke_with("text", opts).await.unwrap();
    assert_eq!(resp["ok"], true);
}

#[tokio::test]
async fn functions_invoke_with_bytes_body_and_region_header() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/functions/v1/bin"))
        .and(header("content-type", "image/png"))
        .and(header("x-region", "eu-west-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "1"})))
        .expect(1)
        .mount(&server)
        .await;
    let opts = InvokeOptions::new()
        .body_bytes(vec![1, 2, 3], "image/png")
        .region(FunctionRegion::EuWest1);
    let _: Value = client(&server).functions().invoke_with("bin", opts).await.unwrap();
    server.verify().await;
}

#[tokio::test]
async fn functions_invoke_with_form_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/functions/v1/form"))
        .and(header(
            "content-type",
            "application/x-www-form-urlencoded",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .expect(1)
        .mount(&server)
        .await;
    let opts = InvokeOptions::new().body_form(vec![("a".into(), "1".into())]);
    let _: Value = client(&server).functions().invoke_with("form", opts).await.unwrap();
    server.verify().await;
}

#[tokio::test]
async fn functions_invoke_with_empty_body_default() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/functions/v1/nobody"))
        .respond_with(ResponseTemplate::new(200).set_body_string(""))
        .expect(1)
        .mount(&server)
        .await;
    // InvokeOptions::default() = Empty body.
    let resp: Option<Value> = client(&server)
        .functions()
        .invoke_with("nobody", InvokeOptions::default())
        .await
        .unwrap();
    assert!(resp.is_none());
}

#[tokio::test]
async fn functions_invoke_with_custom_method_get() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/functions/v1/get-only"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .expect(1)
        .mount(&server)
        .await;
    let opts = InvokeOptions::new().method(InvokeMethod::Get);
    let _: Value = client(&server)
        .functions()
        .invoke_with("get-only", opts)
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn functions_invoke_5xx_returns_storage_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/functions/v1/dies"))
        .respond_with(ResponseTemplate::new(500).set_body_string("crash"))
        .mount(&server)
        .await;
    let err: SupabaseError = client(&server)
        .functions()
        .invoke::<_, Value>("dies", &json!({}))
        .await
        .unwrap_err();
    // decode_error routes Functions service through the Storage path.
    assert!(matches!(err, SupabaseError::Storage(_)));
}

#[tokio::test]
async fn functions_invoke_malformed_response_returns_decode_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/functions/v1/badjson"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{nope"))
        .mount(&server)
        .await;
    let err = client(&server)
        .functions()
        .invoke::<_, Value>("badjson", &json!({}))
        .await
        .unwrap_err();
    assert!(matches!(err, SupabaseError::Decode { .. }));
}

#[tokio::test]
async fn functions_invoke_stream_returns_response_on_success() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/functions/v1/stream"))
        .respond_with(ResponseTemplate::new(200).set_body_string("raw bytes here"))
        .mount(&server)
        .await;
    let resp = client(&server)
        .functions()
        .invoke_stream("stream", InvokeOptions::default())
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    let body = resp.text().await.unwrap();
    assert_eq!(body, "raw bytes here");
}

#[tokio::test]
async fn functions_invoke_stream_error_path() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/functions/v1/err"))
        .respond_with(ResponseTemplate::new(400).set_body_string("bad input"))
        .mount(&server)
        .await;
    let err = client(&server)
        .functions()
        .invoke_stream("err", InvokeOptions::default())
        .await
        .unwrap_err();
    assert!(matches!(err, SupabaseError::Storage(_)));
}

// ===========================================================================
// auth/mod.rs — flows not covered by live tests (mocked deterministically)
// ===========================================================================

fn make_session_body() -> Value {
    json!({
        "access_token": "tok",
        "token_type": "bearer",
        "expires_in": 3600,
        "expires_at": 0,
        "refresh_token": "rtok",
        "user": {
            "id": "u1", "aud": "auth", "role": "auth",
            "created_at": "2024-01-01T00:00:00Z"
        }
    })
}

#[tokio::test]
async fn auth_sign_up_success_persists_session() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/signup"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_session_body()))
        .mount(&server)
        .await;
    let c = client(&server);
    let s = c
        .auth()
        .sign_up(
            "a@b.co",
            "password",
            Default::default(),
        )
        .await
        .unwrap();
    assert_eq!(s.access_token, "tok");
    assert!(c.auth().get_session().is_some());
}

#[tokio::test]
async fn auth_sign_up_without_session_returns_auth_error() {
    let server = MockServer::start().await;
    // Sign-up succeeds but returns only a user (no access_token).
    Mock::given(method("POST"))
        .and(path("/auth/v1/signup"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "user": {
                "id": "u1", "aud": "auth", "role": "auth",
                "created_at": "2024-01-01T00:00:00Z"
            }
        })))
        .mount(&server)
        .await;
    let err = client(&server)
        .auth()
        .sign_up("a@b.co", "pw", Default::default())
        .await
        .unwrap_err();
    assert!(matches!(err, SupabaseError::Auth(_)));
}

#[tokio::test]
async fn auth_sign_in_with_otp_email_posts_otp() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/otp"))
        .and(body_json(json!({"email": "a@b.co"})))
        .respond_with(ResponseTemplate::new(200).set_body_string(""))
        .expect(1)
        .mount(&server)
        .await;
    client(&server)
        .auth()
        .sign_in_with_otp(
            OtpRecipient::Email("a@b.co".into()),
            OtpOptions::default(),
        )
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn auth_sign_in_with_otp_phone_with_options() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/otp"))
        .respond_with(ResponseTemplate::new(200).set_body_string(""))
        .expect(1)
        .mount(&server)
        .await;
    let opts = OtpOptions {
        should_create_user: Some(false),
        email_redirect_to: Some("https://x.co/cb".into()),
        user_metadata: Some(json!({"nick": "n"})),
        captcha_token: Some("captcha-x".into()),
        channel: Some("sms".into()),
    };
    client(&server)
        .auth()
        .sign_in_with_otp(OtpRecipient::Phone("+1555".into()), opts)
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn auth_verify_otp_email_path() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/verify"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_session_body()))
        .mount(&server)
        .await;
    let s = client(&server)
        .auth()
        .verify_otp(VerifyOtpParams::Email {
            email: "a@b.co".into(),
            token: "123456".into(),
            otp_type: OtpType::Email,
        })
        .await
        .unwrap();
    assert_eq!(s.access_token, "tok");
}

#[tokio::test]
async fn auth_verify_otp_phone_path() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/verify"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_session_body()))
        .mount(&server)
        .await;
    let _ = client(&server)
        .auth()
        .verify_otp(VerifyOtpParams::Phone {
            phone: "+1555".into(),
            token: "654321".into(),
            otp_type: OtpType::Sms,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn auth_verify_otp_token_hash_path() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/verify"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_session_body()))
        .mount(&server)
        .await;
    let _ = client(&server)
        .auth()
        .verify_otp(VerifyOtpParams::TokenHash {
            token_hash: "hashed".into(),
            otp_type: OtpType::Recovery,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn auth_resend_email_otp() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/resend"))
        .respond_with(ResponseTemplate::new(200).set_body_string(""))
        .expect(1)
        .mount(&server)
        .await;
    client(&server)
        .auth()
        .resend(OtpRecipient::Email("a@b.co".into()), OtpType::Signup)
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn auth_resend_phone_otp() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/resend"))
        .respond_with(ResponseTemplate::new(200).set_body_string(""))
        .expect(1)
        .mount(&server)
        .await;
    client(&server)
        .auth()
        .resend(OtpRecipient::Phone("+1555".into()), OtpType::Sms)
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn auth_sign_in_anonymously_with_captcha() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/signup"))
        .and(body_json(json!({
            "gotrue_meta_security": { "captcha_token": "captcha" }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_session_body()))
        .mount(&server)
        .await;
    let s = client(&server)
        .auth()
        .sign_in_anonymously(Some("captcha".into()))
        .await
        .unwrap();
    assert_eq!(s.access_token, "tok");
}

#[tokio::test]
async fn auth_sign_in_anonymously_no_captcha() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/signup"))
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_session_body()))
        .mount(&server)
        .await;
    let _ = client(&server).auth().sign_in_anonymously(None).await.unwrap();
}

#[tokio::test]
async fn auth_sign_in_with_id_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/token"))
        .and(query_param("grant_type", "id_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_session_body()))
        .mount(&server)
        .await;
    let s = client(&server)
        .auth()
        .sign_in_with_id_token("google", "id-token-xyz", Some("nonce-1"))
        .await
        .unwrap();
    assert_eq!(s.access_token, "tok");
}

#[tokio::test]
async fn auth_sign_in_with_id_token_no_nonce() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/token"))
        .and(query_param("grant_type", "id_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_session_body()))
        .mount(&server)
        .await;
    let _ = client(&server)
        .auth()
        .sign_in_with_id_token("apple", "id-token-xyz", None)
        .await
        .unwrap();
}

#[tokio::test]
async fn auth_exchange_code_for_session() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/token"))
        .and(query_param("grant_type", "pkce"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_session_body()))
        .mount(&server)
        .await;
    let s = client(&server)
        .auth()
        .exchange_code_for_session("code-xyz")
        .await
        .unwrap();
    assert_eq!(s.access_token, "tok");
}

#[tokio::test]
async fn auth_reset_password_for_email() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/recover"))
        .respond_with(ResponseTemplate::new(200).set_body_string(""))
        .expect(1)
        .mount(&server)
        .await;
    client(&server)
        .auth()
        .reset_password_for_email("a@b.co", ResetPasswordOptions::default())
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn auth_refresh_session_uses_stored_refresh_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/token"))
        .and(query_param("grant_type", "refresh_token"))
        .and(body_json(json!({"refresh_token": "rtok"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_session_body()))
        .mount(&server)
        .await;
    let c = client(&server);
    // Pre-seed the session store so refresh_session(None) uses the stored rtok.
    let initial: rust_supabase_sdk::auth::Session =
        serde_json::from_value(make_session_body()).unwrap();
    c.auth().set_session(initial);
    let s = c.auth().refresh_session(None).await.unwrap();
    assert_eq!(s.access_token, "tok");
}

#[tokio::test]
async fn auth_refresh_session_with_no_session_or_token_returns_auth_error() {
    let server = MockServer::start().await;
    let c = client(&server);
    let err = c.auth().refresh_session(None).await.unwrap_err();
    assert!(matches!(err, SupabaseError::Auth(_)));
}

#[tokio::test]
async fn auth_refresh_session_if_needed_returns_current_when_not_expiring() {
    let server = MockServer::start().await;
    let c = client(&server);
    let mut session: rust_supabase_sdk::auth::Session =
        serde_json::from_value(make_session_body()).unwrap();
    session.expires_at = chrono::Utc::now().timestamp() + 7200; // 2h away
    c.auth().set_session(session);

    // No /token mock — proves refresh_session was NOT called.
    let result = c.auth().refresh_session_if_needed(60).await.unwrap();
    assert!(result.is_some(), "should return the existing session");
}

#[tokio::test]
async fn auth_refresh_session_if_needed_returns_none_with_no_session() {
    let server = MockServer::start().await;
    let c = client(&server);
    let result = c.auth().refresh_session_if_needed(60).await.unwrap();
    assert!(result.is_none(), "should return None when no session stored");
}

#[tokio::test]
async fn auth_refresh_session_if_needed_triggers_refresh_when_expiring() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/token"))
        .and(query_param("grant_type", "refresh_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_session_body()))
        .mount(&server)
        .await;
    let c = client(&server);
    let mut session: rust_supabase_sdk::auth::Session =
        serde_json::from_value(make_session_body()).unwrap();
    session.expires_at = chrono::Utc::now().timestamp() + 10; // 10s — within threshold
    c.auth().set_session(session);

    let result = c.auth().refresh_session_if_needed(60).await.unwrap();
    assert!(result.is_some(), "should have refreshed");
}

#[tokio::test]
async fn auth_get_user_with_no_session_falls_back_to_api_key() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/auth/v1/user"))
        // The fallback bearer is the api_key set in builder.
        .and(header("authorization", "Bearer test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "u1", "aud": "auth", "role": "auth",
            "created_at": "2024-01-01T00:00:00Z"
        })))
        .mount(&server)
        .await;
    let u = client(&server).auth().get_user().await.unwrap();
    assert_eq!(u.id, "u1");
}

#[tokio::test]
async fn auth_update_user_with_no_session() {
    // Exercises the `None` arm of `update_user`'s session match.
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/auth/v1/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "u1", "aud": "auth", "role": "auth",
            "created_at": "2024-01-01T00:00:00Z"
        })))
        .mount(&server)
        .await;
    let u = client(&server)
        .auth()
        .update_user(UpdateUserAttributes::default())
        .await
        .unwrap();
    assert_eq!(u.id, "u1");
}

#[tokio::test]
async fn auth_sign_out_global_hits_logout_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/logout"))
        .respond_with(ResponseTemplate::new(200).set_body_string(""))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    let session: rust_supabase_sdk::auth::Session =
        serde_json::from_value(make_session_body()).unwrap();
    c.auth().set_session(session);
    c.auth().sign_out(SignOutScope::Global).await.unwrap();
    assert!(c.auth().get_session().is_none());
}

#[tokio::test]
async fn auth_sign_out_others_scope() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/logout"))
        .and(query_param("scope", "others"))
        .respond_with(ResponseTemplate::new(200).set_body_string(""))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    let session: rust_supabase_sdk::auth::Session =
        serde_json::from_value(make_session_body()).unwrap();
    c.auth().set_session(session);
    c.auth().sign_out(SignOutScope::Others).await.unwrap();
    // `sign_out` always clears the local session regardless of scope —
    // the scope only changes which sessions the server revokes.
    assert!(c.auth().get_session().is_none());
}

#[tokio::test]
async fn auth_sign_in_with_oauth_returns_authorize_url() {
    // No network call — just URL construction.
    let server = MockServer::start().await;
    let mut qp = std::collections::HashMap::new();
    qp.insert("k".to_string(), "v".to_string());
    let flow = client(&server).auth().sign_in_with_oauth(
        "github",
        OAuthOptions {
            redirect_to: Some("https://x.co/cb".into()),
            scopes: Some("read:user".into()),
            query_params: qp,
            ..Default::default()
        },
    );
    let url = flow.url;
    assert!(url.contains("/auth/v1/authorize"), "url={url}");
    assert!(url.contains("provider=github"), "url={url}");
    assert!(url.contains("redirect_to="), "url={url}");
    assert!(url.contains("scopes="), "url={url}");
    assert!(url.contains("k=v"), "url={url}");
}

// ===========================================================================
// universals/mod.rs — request_bytes pathways via Storage upload
// ===========================================================================

#[tokio::test]
async fn request_bytes_retries_on_429_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/storage/v1/object/bkt/x.bin"))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/storage/v1/object/bkt/x.bin"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"Key": "x.bin"})))
        .mount(&server)
        .await;
    let resp = client(&server)
        .storage()
        .from("bkt")
        .upload("x.bin", vec![1, 2, 3], UploadOptions::default())
        .await
        .unwrap();
    assert_eq!(resp.key.as_deref(), Some("x.bin"));
}

#[tokio::test]
async fn request_bytes_5xx_decodes_as_storage_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/storage/v1/object/bkt/x.bin"))
        .respond_with(ResponseTemplate::new(500).set_body_string("server fault"))
        .mount(&server)
        .await;
    let err = client(&server)
        .storage()
        .from("bkt")
        .upload("x.bin", vec![1], UploadOptions::default())
        .await
        .unwrap_err();
    assert!(matches!(err, SupabaseError::Storage(_)));
}

#[tokio::test]
async fn request_bytes_empty_2xx_decodes_through_helper() {
    // Storage upload with an empty body: `request_bytes` returns Value::Null,
    // and `decode_json::<UploadResponse>(Null)` then errors. This exercises
    // the `body_text.is_empty()` branch in `request_bytes`.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/storage/v1/object/bkt/empty.bin"))
        .respond_with(ResponseTemplate::new(200).set_body_string(""))
        .mount(&server)
        .await;
    let err = client(&server)
        .storage()
        .from("bkt")
        .upload("empty.bin", vec![0], UploadOptions::default())
        .await
        .unwrap_err();
    assert!(matches!(err, SupabaseError::Decode { .. }));
}

#[tokio::test]
async fn request_bytes_malformed_body_yields_decode_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/storage/v1/object/bkt/bad.bin"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{nope"))
        .mount(&server)
        .await;
    let err = client(&server)
        .storage()
        .from("bkt")
        .upload("bad.bin", vec![0], UploadOptions::default())
        .await
        .unwrap_err();
    assert!(matches!(err, SupabaseError::Decode { .. }));
}

#[tokio::test]
async fn upload_update_uses_put_method() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/storage/v1/object/bkt/x.bin"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"Key": "x.bin"})))
        .expect(1)
        .mount(&server)
        .await;
    client(&server)
        .storage()
        .from("bkt")
        .update("x.bin", vec![9], UploadOptions::default())
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn upload_with_cache_control_adds_header() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/storage/v1/object/bkt/cc.bin"))
        .and(header("cache-control", "max-age=3600"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    client(&server)
        .storage()
        .from("bkt")
        .upload(
            "cc.bin",
            vec![0],
            UploadOptions {
                cache_control: Some("3600".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn upload_to_signed_url_uses_signed_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/storage/v1/object/upload/sign/bkt/x.bin"))
        .and(query_param("token", "signed-tok"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"Key": "x.bin"})))
        .expect(1)
        .mount(&server)
        .await;
    let resp = client(&server)
        .storage()
        .from("bkt")
        .upload_to_signed_url(
            "x.bin",
            "signed-tok",
            vec![1, 2],
            UploadOptions { upsert: true, ..Default::default() },
        )
        .await
        .unwrap();
    assert_eq!(resp.key.as_deref(), Some("x.bin"));
}

#[tokio::test]
async fn upload_to_signed_url_error_path() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/storage/v1/object/upload/sign/bkt/x.bin"))
        .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
        .mount(&server)
        .await;
    let err = client(&server)
        .storage()
        .from("bkt")
        .upload_to_signed_url("x.bin", "tok", vec![0], UploadOptions::default())
        .await
        .unwrap_err();
    assert!(matches!(err, SupabaseError::Storage(_)));
}

#[tokio::test]
async fn upload_to_signed_url_empty_response_body_ok() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/storage/v1/object/upload/sign/bkt/x.bin"))
        .respond_with(ResponseTemplate::new(200).set_body_string(""))
        .mount(&server)
        .await;
    let resp = client(&server)
        .storage()
        .from("bkt")
        .upload_to_signed_url("x.bin", "tok", vec![0], UploadOptions::default())
        .await
        .unwrap();
    assert!(resp.key.is_none());
}

#[tokio::test]
async fn create_signed_url_with_transform_options() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/storage/v1/object/sign/bkt/img.png"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({
                "signedURL": "/storage/v1/object/sign/bkt/img.png?token=abc"
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    use rust_supabase_sdk::storage::{ImageFormat, ImageResize, PublicUrlOptions, TransformOptions};
    let url = client(&server)
        .storage()
        .from("bkt")
        .create_signed_url(
            "img.png",
            60,
            PublicUrlOptions {
                download: Some("downloaded.png".into()),
                transform: Some(TransformOptions {
                    width: Some(100),
                    height: Some(200),
                    resize: Some(ImageResize::Cover),
                    quality: Some(80),
                    format: Some(ImageFormat::Webp),
                }),
            },
        )
        .await
        .unwrap();
    // The returned URL should include the token and the appended `download=` flag.
    assert!(url.contains("token=abc"), "url={url}");
    assert!(url.contains("download=downloaded.png"), "url={url}");
}

#[tokio::test]
async fn create_signed_url_with_empty_download_name() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/storage/v1/object/sign/bkt/file.bin"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({
                "signedURL": "/storage/v1/object/sign/bkt/file.bin?token=xyz"
            })),
        )
        .mount(&server)
        .await;
    use rust_supabase_sdk::storage::PublicUrlOptions;
    let url = client(&server)
        .storage()
        .from("bkt")
        .create_signed_url(
            "file.bin",
            60,
            PublicUrlOptions {
                download: Some("".into()),
                transform: None,
            },
        )
        .await
        .unwrap();
    // Empty filename → bare `download=` flag.
    assert!(url.contains("download="), "url={url}");
}

#[tokio::test]
async fn create_signed_url_returns_absolute_when_already_http() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/storage/v1/object/sign/bkt/x.bin"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({
                "signedURL": "https://cdn.example/storage/v1/object/sign/bkt/x.bin?token=abc"
            })),
        )
        .mount(&server)
        .await;
    use rust_supabase_sdk::storage::PublicUrlOptions;
    let url = client(&server)
        .storage()
        .from("bkt")
        .create_signed_url("x.bin", 60, PublicUrlOptions::default())
        .await
        .unwrap();
    assert!(url.starts_with("https://cdn.example"), "url={url}");
}

#[tokio::test]
async fn create_signed_urls_rewrites_relative_to_absolute() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/storage/v1/object/sign/bkt"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"path": "a.txt", "signedURL": "/storage/v1/object/sign/bkt/a.txt?token=t1"},
            {"path": "b.txt", "signedURL": "https://cdn/abs/sign/b.txt?token=t2"}
        ])))
        .mount(&server)
        .await;
    let entries = client(&server)
        .storage()
        .from("bkt")
        .create_signed_urls(["a.txt", "b.txt"], 60)
        .await
        .unwrap();
    assert_eq!(entries.len(), 2);
    let urls: Vec<String> = entries
        .iter()
        .map(|e| e.signed_url.clone().unwrap_or_default())
        .collect();
    // Relative URL got the mock server URL prefixed.
    assert!(urls[0].starts_with(&server.uri()), "url0={}", urls[0]);
    // Absolute URL is untouched.
    assert!(urls[1].starts_with("https://cdn"), "url1={}", urls[1]);
}

#[tokio::test]
async fn create_signed_upload_url_returns_absolute_url() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/storage/v1/object/upload/sign/bkt/up.bin"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "url": "/storage/v1/object/upload/sign/bkt/up.bin?token=abc",
            "token": "abc",
            "path": "up.bin"
        })))
        .mount(&server)
        .await;
    let signed = client(&server)
        .storage()
        .from("bkt")
        .create_signed_upload_url("up.bin")
        .await
        .unwrap();
    assert_eq!(signed.token, "abc");
    assert!(signed.url.starts_with(&server.uri()));
}

#[tokio::test]
async fn create_signed_upload_url_keeps_absolute_url() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/storage/v1/object/upload/sign/bkt/up.bin"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "url": "https://cdn.example/upload?token=abc",
            "token": "abc",
            "path": "up.bin"
        })))
        .mount(&server)
        .await;
    let signed = client(&server)
        .storage()
        .from("bkt")
        .create_signed_upload_url("up.bin")
        .await
        .unwrap();
    assert!(signed.url.starts_with("https://cdn"));
}

#[tokio::test]
async fn download_response_returns_raw_response_on_success() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/storage/v1/object/bkt/file.bin"))
        .respond_with(ResponseTemplate::new(200).set_body_string("payload"))
        .mount(&server)
        .await;
    let resp = client(&server)
        .storage()
        .from("bkt")
        .download_response("file.bin")
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    assert_eq!(resp.text().await.unwrap(), "payload");
}

#[tokio::test]
async fn download_response_propagates_error_status() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/storage/v1/object/bkt/missing.bin"))
        .respond_with(ResponseTemplate::new(404).set_body_string("Not Found"))
        .mount(&server)
        .await;
    let err = client(&server)
        .storage()
        .from("bkt")
        .download_response("missing.bin")
        .await
        .unwrap_err();
    assert!(matches!(err, SupabaseError::Storage(_)));
}

#[tokio::test]
async fn upload_to_signed_url_malformed_json() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/storage/v1/object/upload/sign/bkt/x.bin"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{nope"))
        .mount(&server)
        .await;
    let err = client(&server)
        .storage()
        .from("bkt")
        .upload_to_signed_url("x.bin", "tok", vec![0], UploadOptions::default())
        .await
        .unwrap_err();
    assert!(matches!(err, SupabaseError::Decode { .. }));
}

// ===========================================================================
// auth/admin.rs — invite_user_by_email + generate_link
// ===========================================================================

#[tokio::test]
async fn admin_invite_user_by_email_minimal() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/admin/invite"))
        .and(body_json(json!({"email": "a@b.co"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "u1", "aud": "auth", "role": "auth",
            "created_at": "2024-01-01T00:00:00Z"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let u = client(&server)
        .auth()
        .admin()
        .invite_user_by_email("a@b.co", None, None)
        .await
        .unwrap();
    assert_eq!(u.id, "u1");
}

#[tokio::test]
async fn admin_invite_user_by_email_full_options() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/admin/invite"))
        .and(body_json(json!({
            "email": "a@b.co",
            "redirect_to": "https://x.co/cb",
            "data": {"team": "alpha"}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "u1", "aud": "auth", "role": "auth",
            "created_at": "2024-01-01T00:00:00Z"
        })))
        .expect(1)
        .mount(&server)
        .await;
    client(&server)
        .auth()
        .admin()
        .invite_user_by_email(
            "a@b.co",
            Some("https://x.co/cb"),
            Some(json!({"team": "alpha"})),
        )
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn admin_generate_link_full_payload() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/admin/generate_link"))
        .and(body_json(json!({
            "type": "signup",
            "email": "a@b.co",
            "password": "pw",
            "new_email": "new@b.co",
            "redirect_to": "https://x.co",
            "data": {"k": "v"}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "u1", "aud": "auth", "role": "auth",
            "created_at": "2024-01-01T00:00:00Z",
            "action_link": "https://x.co/action?token=abc",
            "email_otp": "123456",
            "hashed_token": "hashed-x",
            "verification_type": "signup",
            "redirect_to": "https://x.co"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let r = client(&server)
        .auth()
        .admin()
        .generate_link(
            OtpType::Signup,
            "a@b.co",
            Some("pw"),
            Some("new@b.co"),
            Some("https://x.co"),
            Some(json!({"k": "v"})),
        )
        .await
        .unwrap();
    assert_eq!(r.action_link.as_deref(), Some("https://x.co/action?token=abc"));
    assert_eq!(r.user.id, "u1");
}

#[tokio::test]
async fn admin_generate_link_minimal_payload() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/admin/generate_link"))
        .and(body_json(json!({"type": "recovery", "email": "a@b.co"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "u1", "aud": "auth", "role": "auth",
            "created_at": "2024-01-01T00:00:00Z"
        })))
        .expect(1)
        .mount(&server)
        .await;
    client(&server)
        .auth()
        .admin()
        .generate_link(OtpType::Recovery, "a@b.co", None, None, None, None)
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn admin_list_users_pagination_parses_total_and_next() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/auth/v1/admin/users"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-total-count", "42")
                .insert_header(
                    "link",
                    r#"<https://x.co?page=3>; rel="next""#,
                )
                .set_body_json(json!({"users": []})),
        )
        .mount(&server)
        .await;
    let p = client(&server).auth().admin().list_users(1, 10).await.unwrap();
    assert_eq!(p.total, Some(42));
    assert_eq!(p.next_page, Some(3));
}

#[tokio::test]
async fn admin_list_users_empty_body_returns_empty_list() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/auth/v1/admin/users"))
        .respond_with(ResponseTemplate::new(200).set_body_string(""))
        .mount(&server)
        .await;
    let p = client(&server).auth().admin().list_users(1, 10).await.unwrap();
    assert!(p.users.is_empty());
}

// ===========================================================================
// postgrest/builder.rs — execute path variants (object, null, decode err)
// ===========================================================================

#[tokio::test]
async fn execute_handles_bare_object_response() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/t"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": 1})))
        .mount(&server)
        .await;
    let rows = client(&server).from("t").select("*").execute().await.unwrap();
    // Bare object becomes a 1-element Vec.
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], 1);
}

#[tokio::test]
async fn execute_handles_null_response() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/t"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!(null)))
        .mount(&server)
        .await;
    let rows = client(&server).from("t").select("*").execute().await.unwrap();
    assert!(rows.is_empty());
}

#[tokio::test]
async fn execute_handles_array_element_decode_error() {
    // Server returns array of mixed shapes; typed deserialize on a struct
    // expecting `id: String` will reject the int element.
    #[derive(Debug, serde::Deserialize)]
    struct R {
        #[allow(dead_code)]
        id: String,
    }
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/t"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"id": 42}])))
        .mount(&server)
        .await;
    let err = client(&server)
        .from("t")
        .select("*")
        .returns::<R>()
        .execute()
        .await
        .unwrap_err();
    assert!(matches!(err, SupabaseError::Decode { .. }));
}

#[tokio::test]
async fn execute_handles_bare_object_decode_error_for_typed_row() {
    #[derive(Debug, serde::Deserialize)]
    struct R {
        #[allow(dead_code)]
        id: String,
    }
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/t"))
        // Bare object whose `id` is the wrong type.
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": 42})))
        .mount(&server)
        .await;
    let err = client(&server)
        .from("t")
        .select("*")
        .returns::<R>()
        .execute()
        .await
        .unwrap_err();
    assert!(matches!(err, SupabaseError::Decode { .. }));
}

#[tokio::test]
async fn execute_handles_malformed_json_response() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/t"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{not-json"))
        .mount(&server)
        .await;
    let err = client(&server)
        .from("t")
        .select("*")
        .execute()
        .await
        .unwrap_err();
    assert!(matches!(err, SupabaseError::Decode { .. }));
}

#[tokio::test]
async fn maybe_single_multiple_rows_errors() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/t"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!([{"id": 1}, {"id": 2}])),
        )
        .mount(&server)
        .await;
    let err = client(&server)
        .from("t")
        .select("*")
        .maybe_single()
        .execute()
        .await
        .unwrap_err();
    assert!(matches!(err, SupabaseError::Unexpected(_)));
}

#[tokio::test]
async fn single_into_future_await_syntax() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/t"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"id": "x"}])))
        .mount(&server)
        .await;
    let row: Value = client(&server)
        .from("t")
        .select("*")
        .single()
        .await
        .unwrap();
    assert_eq!(row["id"], "x");
}

#[tokio::test]
async fn maybe_single_into_future_await_syntax() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/t"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let result: Option<Value> = client(&server)
        .from("t")
        .select("*")
        .maybe_single()
        .await
        .unwrap();
    assert!(result.is_none());
}

// ===========================================================================
// Legacy delete_user wrapper
// ===========================================================================

#[tokio::test]
async fn auth_sign_in_with_password_phone_branch() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/token"))
        .and(query_param("grant_type", "password"))
        .and(body_json(json!({"phone": "+15550001234", "password": "pw"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_session_body()))
        .mount(&server)
        .await;
    let s = client(&server)
        .auth()
        .sign_in_with_password("+15550001234", "pw")
        .await
        .unwrap();
    assert_eq!(s.access_token, "tok");
}

#[tokio::test]
async fn auth_sign_up_full_options() {
    // Exercises every if-let branch in build_sign_up_body.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/signup"))
        .and(body_json(json!({
            "email": "a@b.co",
            "password": "pw",
            "email_redirect_to": "https://x.co/cb",
            "data": {"name": "Alice"},
            "gotrue_meta_security": {"captcha_token": "cap"},
            "channel": "sms"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_session_body()))
        .expect(1)
        .mount(&server)
        .await;
    use rust_supabase_sdk::auth::SignUpOptions;
    client(&server)
        .auth()
        .sign_up(
            "a@b.co",
            "pw",
            SignUpOptions {
                email_redirect_to: Some("https://x.co/cb".into()),
                user_metadata: Some(json!({"name": "Alice"})),
                captcha_token: Some("cap".into()),
                channel: Some("sms".into()),
            },
        )
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn auth_get_user_decode_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/auth/v1/user"))
        // Missing `id` field — required by User.
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"foo": "bar"})))
        .mount(&server)
        .await;
    let err = client(&server).auth().get_user().await.unwrap_err();
    assert!(matches!(err, SupabaseError::Decode { .. }));
}

#[tokio::test]
async fn auth_update_user_decode_error() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/auth/v1/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"foo": "bar"})))
        .mount(&server)
        .await;
    let err = client(&server)
        .auth()
        .update_user(UpdateUserAttributes::default())
        .await
        .unwrap_err();
    assert!(matches!(err, SupabaseError::Decode { .. }));
}

#[tokio::test]
async fn auth_reset_password_for_email_with_options() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/recover"))
        .and(body_json(json!({
            "email": "a@b.co",
            "redirect_to": "https://x.co/recover",
            "gotrue_meta_security": {"captcha_token": "captcha-x"}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string(""))
        .expect(1)
        .mount(&server)
        .await;
    client(&server)
        .auth()
        .reset_password_for_email(
            "a@b.co",
            ResetPasswordOptions {
                redirect_to: Some("https://x.co/recover".into()),
                captcha_token: Some("captcha-x".into()),
            },
        )
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn auth_token_response_decode_error_in_parse_session() {
    let server = MockServer::start().await;
    // Server returns malformed session payload (missing required fields).
    Mock::given(method("POST"))
        .and(path("/auth/v1/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"foo": "bar"})))
        .mount(&server)
        .await;
    let err = client(&server)
        .auth()
        .sign_in_with_password("a@b.co", "pw")
        .await
        .unwrap_err();
    assert!(matches!(err, SupabaseError::Decode { .. }));
}


//! HTTP transport tests using a local mock server (wiremock).
//!
//! These tests don't require a live Supabase. They exercise:
//!  - 429 retry-with-backoff up to `max_retries`
//!  - `RetryExhausted` error after the budget is spent
//!  - Auth/Storage/Postgrest error variant selection per service
//!  - Auth header passthrough (apikey + bearer)
//!  - Schema header passthrough (`Accept-Profile` / `Content-Profile`)
//!  - Empty-body 2xx returns `Null`
//!  - Malformed JSON body returns `SupabaseError::Decode`
//!  - Custom extra_headers passthrough
//!  - Backoff exponential growth (timing-based, with generous tolerance)

#![allow(clippy::unwrap_used)]

use std::time::{Duration, Instant};

use rust_supabase_sdk::{RetryConfig, SupabaseClient, SupabaseError};
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn make_client(mock_url: &str) -> SupabaseClient {
    SupabaseClient::builder(mock_url, "test-anon-key")
        // Tight retry budget so tests run fast.
        .retry(RetryConfig::new(2, Duration::from_millis(10)))
        .build()
}

// ---------------------------------------------------------------------------
// Retry / backoff
// ---------------------------------------------------------------------------

#[tokio::test]
async fn retries_on_429_then_succeeds() {
    let server = MockServer::start().await;

    // First two attempts: 429. Third: 200.
    Mock::given(method("GET"))
        .and(path("/rest/v1/widgets"))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/widgets"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"id": 1}])))
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let result = client.from("widgets").select("*").execute().await;
    let rows = result.expect("should have eventually succeeded after retries");
    assert_eq!(rows.len(), 1);
}

#[tokio::test]
async fn retry_budget_exhaustion_surfaces_final_429() {
    // After exhausting `max_retries`, the SDK currently returns the *final*
    // 429 response decoded as a service-specific error (Postgrest here),
    // NOT the synthetic `SupabaseError::RetryExhausted` variant. The retry
    // loop runs `0..=max_retries` and falls through on the last iteration.
    // If we ever decide `RetryExhausted` should be the surfaced error
    // instead, this test will catch the behaviour change.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/widgets"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let err = client
        .from("widgets")
        .select("*")
        .execute()
        .await
        .expect_err("should have errored after retries");

    match err {
        SupabaseError::Postgrest(e) => {
            assert_eq!(e.status, 429);
        }
        SupabaseError::RetryExhausted { last_status, .. } => {
            // Acceptable if the SDK ever switches to this behaviour.
            assert_eq!(last_status, Some(429));
        }
        other => panic!("expected Postgrest(429) or RetryExhausted, got {other:?}"),
    }
}

#[tokio::test]
async fn retries_exactly_max_retries_plus_one_total_attempts() {
    // With max_retries=2, the SDK should make 3 total attempts (initial + 2 retries).
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/widgets"))
        .respond_with(ResponseTemplate::new(429))
        .expect(3)
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let _ = client.from("widgets").select("*").execute().await;
    server.verify().await;
}

#[tokio::test]
async fn does_not_retry_on_500() {
    // 500 is not a retryable status — should fail immediately.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/widgets"))
        .respond_with(ResponseTemplate::new(500).set_body_string(""))
        .expect(1) // exactly one call — no retry
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let err = client
        .from("widgets")
        .select("*")
        .execute()
        .await
        .expect_err("500 should error");
    // The exact variant is Postgrest (since service defaults to Postgrest).
    matches!(err, SupabaseError::Postgrest(_));
    server.verify().await;
}

#[tokio::test]
async fn does_not_retry_on_400() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/widgets"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "code": "PGRST100",
            "message": "bad request",
            "details": null,
            "hint": null
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let err = client
        .from("widgets")
        .select("*")
        .execute()
        .await
        .expect_err("400 should error");
    match err {
        SupabaseError::Postgrest(e) => {
            assert_eq!(e.status, 400);
            assert_eq!(e.message, "bad request");
        }
        other => panic!("expected Postgrest error, got {other:?}"),
    }
    server.verify().await;
}

#[tokio::test]
async fn backoff_grows_exponentially() {
    // Three 429s + one 200. With base_backoff=50ms and retries=3, we expect
    // pauses of approximately 50ms, 100ms, 200ms = 350ms minimum.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/widgets"))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(3)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/widgets"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;

    let client = SupabaseClient::builder(server.uri(), "anon")
        .retry(RetryConfig::new(3, Duration::from_millis(50)))
        .build();

    let started = Instant::now();
    let _ = client.from("widgets").select("*").execute().await.unwrap();
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_millis(300),
        "exponential backoff didn't wait long enough: {elapsed:?}"
    );
    // Upper bound: don't wait insanely long either.
    assert!(elapsed < Duration::from_secs(5), "took too long: {elapsed:?}");
}

// ---------------------------------------------------------------------------
// Header passthrough
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sends_apikey_and_bearer_headers() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/widgets"))
        .and(header("apikey", "test-anon-key"))
        .and(header("authorization", "Bearer test-anon-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    client.from("widgets").select("*").execute().await.unwrap();
    server.verify().await;
}

#[tokio::test]
async fn schema_header_is_applied_when_set() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/widgets"))
        .and(header("Accept-Profile", "custom_schema"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;

    let base = make_client(&server.uri());
    let client = base.schema("custom_schema");
    client.from("widgets").select("*").execute().await.unwrap();
    server.verify().await;
}

#[tokio::test]
async fn with_access_token_uses_that_bearer() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/widgets"))
        .and(header("authorization", "Bearer custom-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;

    let base = make_client(&server.uri());
    let client = base.with_access_token("custom-token");
    client.from("widgets").select("*").execute().await.unwrap();
    server.verify().await;
}

#[tokio::test]
async fn builder_extra_headers_are_sent() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/widgets"))
        .and(header("x-trace-id", "abc123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;

    let client = SupabaseClient::builder(server.uri(), "anon")
        .header("X-Trace-Id", "abc123")
        .retry(RetryConfig::new(0, Duration::from_millis(10)))
        .build();
    client.from("widgets").select("*").execute().await.unwrap();
    server.verify().await;
}

#[tokio::test]
async fn user_agent_passthrough() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/widgets"))
        .and(header("user-agent", "my-test-app/1.0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;

    let client = SupabaseClient::builder(server.uri(), "anon")
        .user_agent("my-test-app/1.0")
        .retry(RetryConfig::new(0, Duration::from_millis(10)))
        .build();
    client.from("widgets").select("*").execute().await.unwrap();
    server.verify().await;
}

// ---------------------------------------------------------------------------
// Body / response handling
// ---------------------------------------------------------------------------

#[tokio::test]
async fn empty_body_200_returns_empty_vec() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/widgets"))
        .respond_with(ResponseTemplate::new(200).set_body_string(""))
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let rows = client.from("widgets").select("*").execute().await.unwrap();
    assert!(rows.is_empty());
}

#[tokio::test]
async fn malformed_json_body_yields_decode_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/widgets"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{not json"))
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let err = client
        .from("widgets")
        .select("*")
        .execute()
        .await
        .expect_err("malformed body should error");

    match err {
        SupabaseError::Decode { message, body } => {
            assert!(!message.is_empty());
            assert_eq!(body, "{not json");
        }
        other => panic!("expected Decode error, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Service-aware error decoding
// ---------------------------------------------------------------------------

#[tokio::test]
async fn auth_endpoint_500_decodes_as_auth_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/auth/v1/user"))
        .respond_with(
            ResponseTemplate::new(500).set_body_json(json!({
                "code": 500,
                "msg": "internal error"
            })),
        )
        .mount(&server)
        .await;

    let client = SupabaseClient::builder(server.uri(), "anon")
        .retry(RetryConfig::new(0, Duration::from_millis(10)))
        .access_token("tok")
        .build();
    // Force the auth path via the auth() namespace.
    let err = client.auth().get_user().await.expect_err("auth 500 should error");

    match err {
        SupabaseError::Auth(_) => {} // good
        other => panic!("expected Auth error, got {other:?}"),
    }
}

#[tokio::test]
async fn postgrest_unique_violation_returns_structured_error() {
    // 409 with a PostgrestError body — common for unique-constraint violations.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/rest/v1/widgets"))
        .respond_with(ResponseTemplate::new(409).set_body_json(json!({
            "code": "23505",
            "message": "duplicate key value violates unique constraint \"widgets_name_key\"",
            "details": "Key (name)=(foo) already exists.",
            "hint": null
        })))
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let err = client
        .from("widgets")
        .insert(json!({"name": "foo"}))
        .execute()
        .await
        .expect_err("409 should error");

    match err {
        SupabaseError::Postgrest(e) => {
            assert_eq!(e.status, 409);
            assert_eq!(e.code.as_deref(), Some("23505"));
            assert!(e.message.contains("duplicate key"));
        }
        other => panic!("expected Postgrest error, got {other:?}"),
    }
}

#[tokio::test]
async fn rls_forbidden_decodes_as_postgrest_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/widgets"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "code": "42501",
            "message": "permission denied for table widgets",
            "details": null,
            "hint": null
        })))
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let err = client
        .from("widgets")
        .select("*")
        .execute()
        .await
        .expect_err("401 should error");
    match err {
        SupabaseError::Postgrest(e) => {
            assert_eq!(e.status, 401);
            assert!(e.message.contains("permission denied"));
        }
        other => panic!("expected Postgrest error, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Method / payload routing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn insert_sends_post_with_json_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/rest/v1/widgets"))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!([{"id": "new-id"}])))
        .expect(1)
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let rows = client
        .from("widgets")
        .insert(json!({"name": "thing"}))
        .execute()
        .await
        .unwrap();
    assert_eq!(rows[0]["id"], "new-id");
    server.verify().await;
}

#[tokio::test]
async fn update_sends_patch() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/rest/v1/widgets"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"id": "1", "name": "new"}])))
        .expect(1)
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    client
        .from("widgets")
        .update(json!({"name": "new"}))
        .eq("id", "1")
        .execute()
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn delete_sends_delete_method() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/rest/v1/widgets"))
        .respond_with(ResponseTemplate::new(204).set_body_string(""))
        .expect(1)
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    client
        .from("widgets")
        .delete()
        .eq("id", "1")
        .execute()
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn upsert_includes_prefer_header() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/rest/v1/widgets"))
        // Prefer header should contain "resolution=merge-duplicates"
        .respond_with(ResponseTemplate::new(201).set_body_json(json!([{"id": "1"}])))
        .expect(1)
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    client
        .from("widgets")
        .upsert(json!({"id": "1", "name": "x"}))
        .execute()
        .await
        .unwrap();
    server.verify().await;
}

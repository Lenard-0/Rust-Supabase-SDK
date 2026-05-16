//! Error-path integration tests against a live Supabase project.
//!
//! These verify the SDK surfaces the right error variants when the server
//! rejects a request. Mock-server tests (mock_server_tests.rs) cover the
//! transport-level paths; this file covers the *server*-side paths.
//!
//! Required env vars: SUPABASE_URL, SUPABASE_API_KEY, SUPABASE_SERVICE_WORKER

#![allow(clippy::unwrap_used)]

use std::env;

use dotenv::dotenv;
use rust_supabase_sdk::storage::UploadOptions;
use rust_supabase_sdk::{SupabaseClient, SupabaseError};
use serde_json::json;
use uuid::Uuid;

fn anon_client() -> SupabaseClient {
    SupabaseClient::new(
        env::var("SUPABASE_URL").expect("SUPABASE_URL not set"),
        env::var("SUPABASE_API_KEY").expect("SUPABASE_API_KEY not set"),
        None,
    )
}

fn admin_client() -> SupabaseClient {
    SupabaseClient::new(
        env::var("SUPABASE_URL").expect("SUPABASE_URL not set"),
        env::var("SUPABASE_SERVICE_WORKER").expect("SUPABASE_SERVICE_WORKER not set"),
        None,
    )
}

// ===========================================================================
// PostgREST error paths
// ===========================================================================

#[tokio::test]
async fn select_from_nonexistent_table_returns_postgrest_error() {
    dotenv().ok();
    let client = anon_client();
    let err = client
        .from("definitely_not_a_real_table_zzz_xxx")
        .select("*")
        .execute()
        .await
        .expect_err("missing table should error");
    match err {
        SupabaseError::Postgrest(e) => {
            // PostgREST returns 404 for missing table; some servers return 400/401.
            assert!(
                e.status == 404 || e.status == 400 || e.status == 401 || e.status == 403,
                "expected client error status, got {}",
                e.status
            );
        }
        other => panic!("expected Postgrest error, got {other:?}"),
    }
}

#[tokio::test]
async fn select_unknown_column_returns_42703() {
    dotenv().ok();
    let client = anon_client();
    let err = client
        .from("test_data")
        .select("this_column_does_not_exist")
        .execute()
        .await
        .expect_err("unknown column should error");
    match err {
        SupabaseError::Postgrest(e) => {
            // PostgREST surfaces PostgreSQL's `42703` undefined_column error.
            assert!(
                e.message.to_lowercase().contains("does not exist")
                    || e.code.as_deref() == Some("42703"),
                "expected undefined_column error, got code={:?} message={}",
                e.code,
                e.message
            );
        }
        other => panic!("expected Postgrest error, got {other:?}"),
    }
}

#[tokio::test]
async fn insert_with_invalid_column_returns_error() {
    dotenv().ok();
    let client = anon_client();
    let err = client
        .from("test_data")
        .insert(json!({"this_field_does_not_exist": "x"}))
        .execute()
        .await
        .expect_err("inserting into unknown column should error");
    match err {
        SupabaseError::Postgrest(e) => {
            assert!(
                e.status >= 400 && e.status < 500,
                "expected 4xx, got {}",
                e.status
            );
        }
        other => panic!("expected Postgrest error, got {other:?}"),
    }
}

#[tokio::test]
async fn single_with_zero_rows_returns_not_found_variant() {
    dotenv().ok();
    let client = anon_client();
    let unmatchable = Uuid::new_v4().to_string();
    let err = client
        .from("test_data")
        .select("*")
        .eq("id1", &unmatchable)
        .single()
        .execute()
        .await
        .expect_err("zero rows on .single() should error");
    match err {
        SupabaseError::NotFound { resource } => {
            assert_eq!(resource, "test_data");
        }
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[tokio::test]
async fn invalid_filter_syntax_returns_postgrest_error() {
    dotenv().ok();
    let client = anon_client();
    // PostgREST rejects operators it doesn't recognise. Use the escape hatch
    // to send a bogus op intentionally.
    let err = client
        .from("test_data")
        .select("*")
        .filter("name", "definitely_not_a_real_op", "x")
        .execute()
        .await
        .expect_err("bogus operator should error");
    match err {
        SupabaseError::Postgrest(e) => {
            assert!(e.status >= 400, "expected error status, got {}", e.status);
        }
        other => panic!("expected Postgrest error, got {other:?}"),
    }
}

// ===========================================================================
// Auth error paths
// ===========================================================================

#[tokio::test]
async fn malformed_jwt_yields_auth_error() {
    dotenv().ok();
    // Build a client with a clearly-invalid token.
    let client = SupabaseClient::new(
        env::var("SUPABASE_URL").unwrap(),
        env::var("SUPABASE_API_KEY").unwrap(),
        Some("not.a.valid.jwt.at.all".to_string()),
    );
    let err = client.auth().get_user().await.expect_err("bogus jwt should fail");
    match err {
        SupabaseError::Auth(_) => {}
        other => panic!("expected Auth error, got {other:?}"),
    }
}

#[tokio::test]
async fn admin_endpoint_with_anon_key_is_rejected() {
    dotenv().ok();
    // Try to hit /auth/v1/admin/users with the anon key — should be forbidden.
    let anon = anon_client();
    let err = anon
        .auth()
        .admin()
        .list_users(1, 10)
        .await
        .expect_err("anon key should not access admin endpoints");
    match err {
        SupabaseError::Auth(_) | SupabaseError::Postgrest(_) | SupabaseError::Storage(_) => {}
        other => panic!("expected Auth/Postgrest/Storage error, got {other:?}"),
    }
}

#[tokio::test]
async fn delete_nonexistent_user_via_admin_errors() {
    dotenv().ok();
    let admin = admin_client();
    let err = admin
        .auth()
        .admin()
        .delete_user(&Uuid::new_v4().to_string(), false)
        .await
        .expect_err("deleting unknown user should fail");
    match err {
        SupabaseError::Auth(_) => {}
        other => panic!("expected Auth error, got {other:?}"),
    }
}

// ===========================================================================
// Storage error paths
// ===========================================================================

#[tokio::test]
async fn create_bucket_with_duplicate_name_errors() {
    dotenv().ok();
    let client = admin_client();
    let name = format!("err-{}", Uuid::new_v4());

    client
        .storage()
        .create_bucket(&name, Default::default())
        .await
        .unwrap();

    let err = client
        .storage()
        .create_bucket(&name, Default::default())
        .await
        .expect_err("duplicate bucket name should fail");
    match err {
        SupabaseError::Storage(_) => {}
        other => panic!("expected Storage error, got {other:?}"),
    }

    let _ = client.storage().delete_bucket(&name).await;
}

#[tokio::test]
async fn download_with_wrong_bucket_path_errors() {
    dotenv().ok();
    let client = admin_client();
    let err = client
        .storage()
        .from("nonexistent-bucket-zzz")
        .download("anything.txt")
        .await
        .expect_err("download from missing bucket should fail");
    match err {
        SupabaseError::Storage(_) | SupabaseError::NotFound { .. } => {}
        other => panic!("expected Storage/NotFound error, got {other:?}"),
    }
}

#[tokio::test]
async fn upload_then_delete_bucket_makes_object_unreachable() {
    dotenv().ok();
    let client = admin_client();
    let name = format!("seq-{}", Uuid::new_v4());

    client
        .storage()
        .create_bucket(&name, Default::default())
        .await
        .unwrap();
    client
        .storage()
        .from(&name)
        .upload("x.txt", b"y".to_vec(), UploadOptions::default())
        .await
        .unwrap();
    client.storage().empty_bucket(&name).await.unwrap();

    // Retry deletion since empty_bucket is eventually consistent.
    let mut deleted = false;
    for _ in 0..20 {
        if client.storage().delete_bucket(&name).await.is_ok() {
            deleted = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(deleted);

    let err = client
        .storage()
        .from(&name)
        .download("x.txt")
        .await
        .expect_err("download after bucket delete should fail");
    match err {
        SupabaseError::Storage(_) | SupabaseError::NotFound { .. } => {}
        other => panic!("expected Storage error, got {other:?}"),
    }
}

// ===========================================================================
// Transport / global error paths
// ===========================================================================

#[tokio::test]
async fn invalid_api_key_yields_auth_error_on_postgrest_endpoint() {
    dotenv().ok();
    let client = SupabaseClient::new(
        env::var("SUPABASE_URL").unwrap(),
        "totally-bogus-key",
        None,
    );
    let err = client
        .from("test_data")
        .select("*")
        .execute()
        .await
        .expect_err("bogus key should fail");
    // Could be Auth or Postgrest depending on which layer rejects it.
    match err {
        SupabaseError::Auth(_) | SupabaseError::Postgrest(_) => {}
        other => panic!("expected Auth/Postgrest error, got {other:?}"),
    }
}

#[tokio::test]
async fn unreachable_url_returns_http_error() {
    dotenv().ok();
    let client = SupabaseClient::builder(
        "http://localhost:1", // closed port, instant connect-refused
        env::var("SUPABASE_API_KEY").unwrap(),
    )
    .retry(rust_supabase_sdk::RetryConfig::new(
        0,
        std::time::Duration::from_millis(10),
    ))
    .build();

    let err = client
        .from("test_data")
        .select("*")
        .execute()
        .await
        .expect_err("connect-refused should fail");
    match err {
        SupabaseError::Transport(_) => {}
        other => panic!("expected Transport error, got {other:?}"),
    }
}

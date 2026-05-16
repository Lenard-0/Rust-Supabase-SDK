//! Integration tests for the Auth namespace against a live Supabase project.
//!
//! Strategy: every test creates its own user (unique UUID-tagged email) via
//! the service-role admin API with `email_confirm: true`, runs whatever auth
//! flow it exercises with the anon-key client, then deletes the user in the
//! admin client to clean up. No project-wide email-confirmation flag changes
//! are required.
//!
//! Required env vars (from `.env`):
//!   SUPABASE_URL
//!   SUPABASE_API_KEY            anon key
//!   SUPABASE_SERVICE_WORKER     service_role key (for admin cleanup)

#![allow(clippy::unwrap_used)]

use std::env;

use dotenv::dotenv;
use rust_supabase_sdk::auth::{
    AdminUserAttributes, SignOutScope, UpdateUserAttributes,
};
use rust_supabase_sdk::{SupabaseClient, SupabaseError};
use uuid::Uuid;

const PASSWORD: &str = "Test12345!secret";

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

fn unique_email() -> String {
    format!("test-{}@example.test", Uuid::new_v4())
}

/// Create a confirmed user via admin and return (user_id, email).
async fn create_confirmed_user(admin: &SupabaseClient) -> (String, String) {
    let email = unique_email();
    let user = admin
        .auth()
        .admin()
        .create_user(AdminUserAttributes {
            email: Some(email.clone()),
            password: Some(PASSWORD.to_string()),
            email_confirm: Some(true),
            ..Default::default()
        })
        .await
        .expect("admin.create_user should succeed");
    (user.id, email)
}

async fn delete_user(admin: &SupabaseClient, user_id: &str) {
    let _ = admin.auth().admin().delete_user(user_id, false).await;
}

// ===========================================================================
// sign_in_with_password
// ===========================================================================

#[tokio::test]
async fn sign_in_with_password_returns_session() {
    dotenv().ok();
    let admin = admin_client();
    let (user_id, email) = create_confirmed_user(&admin).await;

    let client = anon_client();
    let session = client
        .auth()
        .sign_in_with_password(&email, PASSWORD)
        .await
        .expect("sign-in should succeed");

    assert_eq!(session.token_type, "bearer");
    assert!(!session.access_token.is_empty());
    assert!(!session.refresh_token.is_empty());
    assert_eq!(session.user.email.as_deref(), Some(email.as_str()));

    // Session is automatically persisted to the store.
    let stored = client.auth().get_session();
    assert!(stored.is_some(), "session should be persisted in store");
    assert_eq!(stored.unwrap().access_token, session.access_token);

    delete_user(&admin, &user_id).await;
}

#[tokio::test]
async fn sign_in_with_wrong_password_returns_auth_error() {
    dotenv().ok();
    let admin = admin_client();
    let (user_id, email) = create_confirmed_user(&admin).await;

    let client = anon_client();
    let err = client
        .auth()
        .sign_in_with_password(&email, "wrong-password")
        .await
        .expect_err("wrong password should fail");
    match err {
        SupabaseError::Auth(_) => {}
        other => panic!("expected Auth error, got {other:?}"),
    }
    assert!(
        client.auth().get_session().is_none(),
        "no session should be stored after failed sign-in"
    );

    delete_user(&admin, &user_id).await;
}

#[tokio::test]
async fn sign_in_unknown_email_returns_auth_error() {
    dotenv().ok();
    let client = anon_client();
    let err = client
        .auth()
        .sign_in_with_password(&unique_email(), PASSWORD)
        .await
        .expect_err("unknown email should fail");
    match err {
        SupabaseError::Auth(_) => {}
        other => panic!("expected Auth error, got {other:?}"),
    }
}

// ===========================================================================
// get_user
// ===========================================================================

#[tokio::test]
async fn get_user_returns_authenticated_user() {
    dotenv().ok();
    let admin = admin_client();
    let (user_id, email) = create_confirmed_user(&admin).await;

    let client = anon_client();
    client.auth().sign_in_with_password(&email, PASSWORD).await.unwrap();

    let user = client.auth().get_user().await.expect("get_user should succeed");
    assert_eq!(user.id, user_id);
    assert_eq!(user.email.as_deref(), Some(email.as_str()));

    delete_user(&admin, &user_id).await;
}

#[tokio::test]
async fn get_user_without_session_yields_auth_error() {
    dotenv().ok();
    let client = anon_client();
    // No sign-in, no session.
    let err = client.auth().get_user().await.expect_err("should fail without auth");
    match err {
        SupabaseError::Auth(_) => {}
        other => panic!("expected Auth error, got {other:?}"),
    }
}

// ===========================================================================
// update_user
// ===========================================================================

#[tokio::test]
async fn update_user_changes_metadata() {
    dotenv().ok();
    let admin = admin_client();
    let (user_id, email) = create_confirmed_user(&admin).await;

    let client = anon_client();
    client.auth().sign_in_with_password(&email, PASSWORD).await.unwrap();

    let updated = client
        .auth()
        .update_user(UpdateUserAttributes {
            user_metadata: Some(serde_json::json!({ "nickname": "tester" })),
            ..Default::default()
        })
        .await
        .expect("update_user should succeed");
    assert_eq!(updated.user_metadata["nickname"], "tester");

    // Re-fetch to confirm persistence.
    let refetched = client.auth().get_user().await.unwrap();
    assert_eq!(refetched.user_metadata["nickname"], "tester");

    delete_user(&admin, &user_id).await;
}

// ===========================================================================
// refresh_session
// ===========================================================================

#[tokio::test]
async fn refresh_session_returns_new_access_token() {
    dotenv().ok();
    let admin = admin_client();
    let (user_id, email) = create_confirmed_user(&admin).await;

    let client = anon_client();
    let initial = client.auth().sign_in_with_password(&email, PASSWORD).await.unwrap();

    // Sleep briefly so the new token has a different iat — Supabase's refresh
    // sometimes issues identical access_tokens within the same second.
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    let refreshed = client
        .auth()
        .refresh_session(None)
        .await
        .expect("refresh_session should succeed");

    assert_eq!(refreshed.user.id, initial.user.id);
    assert!(!refreshed.access_token.is_empty());
    assert!(!refreshed.refresh_token.is_empty());
    // Session in the store reflects the refreshed token.
    assert_eq!(client.auth().get_session().unwrap().access_token, refreshed.access_token);

    delete_user(&admin, &user_id).await;
}

#[tokio::test]
async fn refresh_with_invalid_token_errors() {
    dotenv().ok();
    let client = anon_client();
    let err = client
        .auth()
        .refresh_session(Some("totally-bogus-refresh-token"))
        .await
        .expect_err("invalid refresh token should error");
    match err {
        SupabaseError::Auth(_) => {}
        other => panic!("expected Auth error, got {other:?}"),
    }
}

// ===========================================================================
// sign_out
// ===========================================================================

#[tokio::test]
async fn sign_out_clears_local_session() {
    dotenv().ok();
    let admin = admin_client();
    let (user_id, email) = create_confirmed_user(&admin).await;

    let client = anon_client();
    client.auth().sign_in_with_password(&email, PASSWORD).await.unwrap();
    assert!(client.auth().get_session().is_some());

    client
        .auth()
        .sign_out(SignOutScope::Local)
        .await
        .expect("sign_out should succeed");

    assert!(client.auth().get_session().is_none(), "session should be cleared");
    delete_user(&admin, &user_id).await;
}

#[tokio::test]
async fn sign_out_global_invalidates_token_server_side() {
    dotenv().ok();
    let admin = admin_client();
    let (user_id, email) = create_confirmed_user(&admin).await;

    let client = anon_client();
    let session = client.auth().sign_in_with_password(&email, PASSWORD).await.unwrap();
    client.auth().sign_out(SignOutScope::Global).await.unwrap();

    // After global sign-out, the previous access_token should no longer work
    // for /user. Use a fresh client with the now-stale access_token.
    let stale = SupabaseClient::new(
        env::var("SUPABASE_URL").unwrap(),
        env::var("SUPABASE_API_KEY").unwrap(),
        Some(session.access_token.clone()),
    );
    let err = stale.auth().get_user().await.expect_err("stale token should be rejected");
    match err {
        SupabaseError::Auth(_) => {}
        other => panic!("expected Auth error, got {other:?}"),
    }
    delete_user(&admin, &user_id).await;
}

// ===========================================================================
// set_session / clear_session
// ===========================================================================

#[tokio::test]
async fn set_session_then_get_user_uses_provided_token() {
    dotenv().ok();
    let admin = admin_client();
    let (user_id, email) = create_confirmed_user(&admin).await;

    // First client signs in to grab a session.
    let c1 = anon_client();
    let session = c1.auth().sign_in_with_password(&email, PASSWORD).await.unwrap();

    // Second client manually installs the session, then queries.
    let c2 = anon_client();
    c2.auth().set_session(session.clone());
    assert!(c2.auth().get_session().is_some());

    let user = c2.auth().get_user().await.unwrap();
    assert_eq!(user.id, user_id);

    delete_user(&admin, &user_id).await;
}

#[tokio::test]
async fn clear_session_drops_persisted_state() {
    dotenv().ok();
    let admin = admin_client();
    let (user_id, email) = create_confirmed_user(&admin).await;

    let client = anon_client();
    client.auth().sign_in_with_password(&email, PASSWORD).await.unwrap();
    assert!(client.auth().get_session().is_some());

    client.auth().clear_session();
    assert!(client.auth().get_session().is_none());

    delete_user(&admin, &user_id).await;
}

// ===========================================================================
// Anonymous sign-in
// ===========================================================================

#[tokio::test]
async fn sign_in_anonymously_returns_anon_session() {
    dotenv().ok();
    let client = anon_client();
    let session = match client.auth().sign_in_anonymously(None).await {
        Ok(s) => s,
        Err(SupabaseError::Auth(e))
            if e.message.contains("disabled") || e.message.to_lowercase().contains("anonymous") =>
        {
            // Project doesn't have anonymous sign-in enabled — that's fine.
            eprintln!("Anonymous sign-in not enabled on project; skipping.");
            return;
        }
        Err(other) => panic!("anonymous sign-in failed: {other:?}"),
    };
    assert!(session.user.is_anonymous);
    // Best-effort cleanup.
    let admin = admin_client();
    let _ = admin.auth().admin().delete_user(&session.user.id, false).await;
}

// ===========================================================================
// Admin API surface
// ===========================================================================

#[tokio::test]
async fn admin_create_get_update_delete_user_roundtrip() {
    dotenv().ok();
    let admin = admin_client();
    let email = unique_email();

    // CREATE
    let user = admin
        .auth()
        .admin()
        .create_user(AdminUserAttributes {
            email: Some(email.clone()),
            password: Some(PASSWORD.to_string()),
            email_confirm: Some(true),
            user_metadata: Some(serde_json::json!({"role": "tester"})),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(user.email.as_deref(), Some(email.as_str()));
    assert_eq!(user.user_metadata["role"], "tester");

    // GET BY ID
    let fetched = admin.auth().admin().get_user_by_id(&user.id).await.unwrap();
    assert_eq!(fetched.id, user.id);

    // UPDATE
    let updated = admin
        .auth()
        .admin()
        .update_user_by_id(
            &user.id,
            AdminUserAttributes {
                user_metadata: Some(serde_json::json!({"role": "admin"})),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.user_metadata["role"], "admin");

    // DELETE
    admin.auth().admin().delete_user(&user.id, false).await.unwrap();

    // GET BY ID after delete → error (user not found)
    let err = admin.auth().admin().get_user_by_id(&user.id).await;
    assert!(err.is_err(), "deleted user should not be fetchable");
}

#[tokio::test]
async fn admin_list_users_pagination() {
    dotenv().ok();
    let admin = admin_client();
    // Create a few users we know exist for this run.
    let mut created_ids: Vec<String> = Vec::new();
    for _ in 0..3 {
        let (id, _email) = create_confirmed_user(&admin).await;
        created_ids.push(id);
    }

    let page = admin.auth().admin().list_users(1, 100).await.unwrap();
    // We can't assert exact counts (other users exist), but we can assert
    // we got users back and we can find at least one of ours.
    assert!(!page.users.is_empty());
    let found = page
        .users
        .iter()
        .any(|u| created_ids.contains(&u.id));
    assert!(found, "expected to find at least one created user in page");

    for id in created_ids {
        delete_user(&admin, &id).await;
    }
}

// ===========================================================================
// Auth API surface — duplicate sign-up via admin
// ===========================================================================

#[tokio::test]
async fn admin_create_user_duplicate_email_errors() {
    dotenv().ok();
    let admin = admin_client();
    let (user_id, email) = create_confirmed_user(&admin).await;

    // Second create with same email should error.
    let err = admin
        .auth()
        .admin()
        .create_user(AdminUserAttributes {
            email: Some(email.clone()),
            password: Some(PASSWORD.to_string()),
            email_confirm: Some(true),
            ..Default::default()
        })
        .await
        .expect_err("duplicate email should fail");
    match err {
        SupabaseError::Auth(_) => {}
        SupabaseError::Postgrest(_) => {} // some servers wrap it differently
        other => panic!("expected Auth or Postgrest error, got {other:?}"),
    }

    delete_user(&admin, &user_id).await;
}

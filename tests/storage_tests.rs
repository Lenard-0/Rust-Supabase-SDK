//! End-to-end integration tests for the Storage namespace.
//!
//! Strategy: each test creates its own unique bucket via the service-role key,
//! exercises one slice of the API, then deletes the bucket on the way out so
//! tests in parallel can't tread on each other.
//!
//! This avoids needing the user to pre-provision named buckets like
//! `test-public-bucket` / `test-private-bucket` — every test owns its scratch
//! bucket end-to-end.
//!
//! Required env vars:
//!   SUPABASE_URL
//!   SUPABASE_SERVICE_WORKER

#![allow(clippy::unwrap_used)]

use std::env;

use dotenv::dotenv;
use rust_supabase_sdk::storage::{
    CreateBucketOptions, ListOptions, PublicUrlOptions, SortColumn, SortOrder, UpdateBucketOptions,
    UploadOptions,
};
use rust_supabase_sdk::{SupabaseClient, SupabaseError};
use uuid::Uuid;

fn admin_client() -> SupabaseClient {
    SupabaseClient::new(
        env::var("SUPABASE_URL").expect("SUPABASE_URL not set"),
        env::var("SUPABASE_SERVICE_WORKER").expect("SUPABASE_SERVICE_WORKER not set"),
        None,
    )
}

fn unique_bucket() -> String {
    // Bucket names can't start with a digit; UUIDs sometimes do. Prefix with `t-`.
    format!("t-{}", Uuid::new_v4())
}

/// Create a bucket of the given visibility, returning a scoped helper that
/// removes the bucket on drop... actually we can't async-drop, so use the
/// `BucketGuard` pattern with an explicit `.cleanup()` await at the end.
struct BucketGuard {
    client: SupabaseClient,
    name: String,
}

impl BucketGuard {
    async fn create(client: SupabaseClient, public: bool) -> Self {
        let name = unique_bucket();
        client
            .storage()
            .create_bucket(
                &name,
                CreateBucketOptions { public, ..Default::default() },
            )
            .await
            .expect("create_bucket should succeed");
        Self { client, name }
    }

    async fn cleanup(self) {
        // Best-effort: empty and remove. Don't panic on cleanup errors.
        let _ = self.client.storage().empty_bucket(&self.name).await;
        let _ = self.client.storage().delete_bucket(&self.name).await;
    }
}

// ===========================================================================
// Bucket CRUD
// ===========================================================================

#[tokio::test]
async fn list_buckets_includes_a_newly_created_one() {
    dotenv().ok();
    let client = admin_client();
    let bucket = BucketGuard::create(client.clone(), false).await;

    let all = client.storage().list_buckets().await.unwrap();
    let found = all.iter().any(|b| b.id == bucket.name || b.name == bucket.name);
    assert!(found, "newly created bucket {} not found in list_buckets", bucket.name);

    bucket.cleanup().await;
}

#[tokio::test]
async fn create_get_update_delete_bucket_roundtrip() {
    dotenv().ok();
    let client = admin_client();
    let name = unique_bucket();

    // CREATE
    let returned = client
        .storage()
        .create_bucket(
            &name,
            CreateBucketOptions { public: false, ..Default::default() },
        )
        .await
        .unwrap();
    assert!(returned == name || returned.contains(&name));

    // GET
    let got = client.storage().get_bucket(&name).await.unwrap();
    assert_eq!(got.id, name);
    assert!(!got.public);

    // UPDATE: make it public
    client
        .storage()
        .update_bucket(&name, UpdateBucketOptions {
            public: Some(true),
            ..Default::default()
        })
        .await
        .unwrap();
    let after_update = client.storage().get_bucket(&name).await.unwrap();
    assert!(after_update.public, "bucket should now be public");

    // DELETE
    client.storage().delete_bucket(&name).await.unwrap();
    let err = client.storage().get_bucket(&name).await;
    assert!(err.is_err(), "deleted bucket should not be fetchable");
}

#[tokio::test]
async fn get_bucket_not_found_returns_error() {
    dotenv().ok();
    let client = admin_client();
    let err = client
        .storage()
        .get_bucket("definitely-does-not-exist-zzz-zzz")
        .await
        .expect_err("should fail");
    match err {
        SupabaseError::Storage(_) | SupabaseError::NotFound { .. } => {}
        other => panic!("expected Storage / NotFound error, got {other:?}"),
    }
}

// ===========================================================================
// Object CRUD
// ===========================================================================

#[tokio::test]
async fn upload_download_roundtrip_preserves_bytes() {
    dotenv().ok();
    let client = admin_client();
    let bucket = BucketGuard::create(client.clone(), false).await;
    let payload = b"hello world from a test\n".to_vec();
    let path = "greeting.txt";

    client
        .storage()
        .from(&bucket.name)
        .upload(
            path,
            payload.clone(),
            UploadOptions {
                content_type: Some("text/plain".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let downloaded = client.storage().from(&bucket.name).download(path).await.unwrap();
    assert_eq!(downloaded, payload);

    bucket.cleanup().await;
}

#[tokio::test]
async fn upload_without_upsert_rejects_existing_path() {
    dotenv().ok();
    let client = admin_client();
    let bucket = BucketGuard::create(client.clone(), false).await;
    let path = "same.txt";

    client.storage().from(&bucket.name)
        .upload(path, b"first".to_vec(), UploadOptions::default())
        .await
        .unwrap();

    let err = client.storage().from(&bucket.name)
        .upload(path, b"second".to_vec(), UploadOptions::default())
        .await
        .expect_err("second upload without upsert should fail");
    match err {
        SupabaseError::Storage(_) => {}
        other => panic!("expected Storage error, got {other:?}"),
    }

    bucket.cleanup().await;
}

#[tokio::test]
async fn upload_with_upsert_overwrites_existing() {
    dotenv().ok();
    let client = admin_client();
    let bucket = BucketGuard::create(client.clone(), false).await;
    let path = "ow.txt";

    client.storage().from(&bucket.name)
        .upload(path, b"v1".to_vec(), UploadOptions::default())
        .await
        .unwrap();

    client.storage().from(&bucket.name)
        .upload(
            path,
            b"v2".to_vec(),
            UploadOptions { upsert: true, ..Default::default() },
        )
        .await
        .unwrap();

    let bytes = client.storage().from(&bucket.name).download(path).await.unwrap();
    assert_eq!(bytes, b"v2");

    bucket.cleanup().await;
}

#[tokio::test]
async fn update_replaces_existing_object() {
    dotenv().ok();
    let client = admin_client();
    let bucket = BucketGuard::create(client.clone(), false).await;
    let path = "thing.bin";

    client.storage().from(&bucket.name)
        .upload(path, vec![1, 2, 3], UploadOptions::default())
        .await
        .unwrap();
    client.storage().from(&bucket.name)
        .update(path, vec![9, 9, 9], UploadOptions::default())
        .await
        .unwrap();

    let bytes = client.storage().from(&bucket.name).download(path).await.unwrap();
    assert_eq!(bytes, vec![9, 9, 9]);

    bucket.cleanup().await;
}

#[tokio::test]
async fn remove_deletes_objects() {
    dotenv().ok();
    let client = admin_client();
    let bucket = BucketGuard::create(client.clone(), false).await;

    client.storage().from(&bucket.name)
        .upload("a.txt", b"a".to_vec(), UploadOptions::default())
        .await
        .unwrap();
    client.storage().from(&bucket.name)
        .upload("b.txt", b"b".to_vec(), UploadOptions::default())
        .await
        .unwrap();

    let removed = client.storage().from(&bucket.name)
        .remove(["a.txt", "b.txt"])
        .await
        .unwrap();
    assert!(!removed.is_empty(), "remove should report deleted files");

    // Downloads should now fail.
    let err = client.storage().from(&bucket.name).download("a.txt").await;
    assert!(err.is_err());

    bucket.cleanup().await;
}

// ===========================================================================
// list
// ===========================================================================

#[tokio::test]
async fn list_returns_uploaded_objects() {
    dotenv().ok();
    let client = admin_client();
    let bucket = BucketGuard::create(client.clone(), false).await;

    for name in ["x1.txt", "x2.txt", "x3.txt"] {
        client.storage().from(&bucket.name)
            .upload(name, b"data".to_vec(), UploadOptions::default())
            .await
            .unwrap();
    }

    let files = client.storage().from(&bucket.name)
        .list("", ListOptions::default())
        .await
        .unwrap();
    assert!(files.len() >= 3, "expected at least 3 files, got {}", files.len());
    let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"x1.txt"));
    assert!(names.contains(&"x2.txt"));
    assert!(names.contains(&"x3.txt"));

    bucket.cleanup().await;
}

#[tokio::test]
async fn list_with_sort_and_limit() {
    dotenv().ok();
    let client = admin_client();
    let bucket = BucketGuard::create(client.clone(), false).await;

    for name in ["alpha.txt", "beta.txt", "gamma.txt"] {
        client.storage().from(&bucket.name)
            .upload(name, b"x".to_vec(), UploadOptions::default())
            .await
            .unwrap();
    }

    let files = client.storage().from(&bucket.name)
        .list("", ListOptions {
            limit: Some(2),
            sort_by: Some((SortColumn::Name, SortOrder::Asc)),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(files.len(), 2);
    // sorted ascending by name => alpha, beta
    assert_eq!(files[0].name, "alpha.txt");
    assert_eq!(files[1].name, "beta.txt");

    bucket.cleanup().await;
}

// ===========================================================================
// move / copy
// ===========================================================================

#[tokio::test]
async fn move_relocates_object() {
    dotenv().ok();
    let client = admin_client();
    let bucket = BucketGuard::create(client.clone(), false).await;

    client.storage().from(&bucket.name)
        .upload("orig.txt", b"data".to_vec(), UploadOptions::default())
        .await
        .unwrap();

    client.storage().from(&bucket.name)
        .move_("orig.txt", "moved.txt")
        .await
        .unwrap();

    // Original is gone.
    assert!(client.storage().from(&bucket.name).download("orig.txt").await.is_err());
    // New path has the content.
    let bytes = client.storage().from(&bucket.name).download("moved.txt").await.unwrap();
    assert_eq!(bytes, b"data");

    bucket.cleanup().await;
}

#[tokio::test]
async fn copy_duplicates_object() {
    dotenv().ok();
    let client = admin_client();
    let bucket = BucketGuard::create(client.clone(), false).await;

    client.storage().from(&bucket.name)
        .upload("src.txt", b"hello".to_vec(), UploadOptions::default())
        .await
        .unwrap();

    client.storage().from(&bucket.name)
        .copy("src.txt", "dst.txt")
        .await
        .unwrap();

    let src = client.storage().from(&bucket.name).download("src.txt").await.unwrap();
    let dst = client.storage().from(&bucket.name).download("dst.txt").await.unwrap();
    assert_eq!(src, b"hello");
    assert_eq!(dst, b"hello");

    bucket.cleanup().await;
}

// ===========================================================================
// Signed URLs
// ===========================================================================

#[tokio::test]
async fn create_signed_url_for_existing_object() {
    dotenv().ok();
    let client = admin_client();
    let bucket = BucketGuard::create(client.clone(), false).await;

    client.storage().from(&bucket.name)
        .upload("secret.txt", b"shh".to_vec(), UploadOptions::default())
        .await
        .unwrap();

    let signed = client.storage().from(&bucket.name)
        .create_signed_url("secret.txt", 60, PublicUrlOptions::default())
        .await
        .unwrap();
    // Should be a URL containing the bucket name and a `token=` query param.
    assert!(signed.contains("secret.txt") || signed.contains("token="), "url={signed}");
    assert!(signed.starts_with("http") || signed.starts_with("/"), "unexpected url={signed}");

    bucket.cleanup().await;
}

#[tokio::test]
async fn create_signed_urls_batch() {
    dotenv().ok();
    let client = admin_client();
    let bucket = BucketGuard::create(client.clone(), false).await;

    for name in ["one.txt", "two.txt"] {
        client.storage().from(&bucket.name)
            .upload(name, b"x".to_vec(), UploadOptions::default())
            .await
            .unwrap();
    }

    let entries = client.storage().from(&bucket.name)
        .create_signed_urls(["one.txt", "two.txt"], 60)
        .await
        .unwrap();
    assert_eq!(entries.len(), 2);
    for e in &entries {
        assert!(e.error.is_none(), "expected no error, got {:?}", e.error);
        assert!(e.path.is_some());
    }

    bucket.cleanup().await;
}

// ===========================================================================
// Public URL (no network call)
// ===========================================================================

#[tokio::test]
async fn get_public_url_builds_path() {
    dotenv().ok();
    let client = admin_client();
    let bucket = BucketGuard::create(client.clone(), true).await;

    let url = client.storage().from(&bucket.name)
        .get_public_url("img.png", PublicUrlOptions::default());
    assert!(url.contains(&bucket.name), "url={url}");
    assert!(url.contains("img.png"), "url={url}");
    assert!(url.starts_with("http"), "url={url}");

    bucket.cleanup().await;
}

// ===========================================================================
// Error paths
// ===========================================================================

#[tokio::test]
async fn download_nonexistent_object_errors() {
    dotenv().ok();
    let client = admin_client();
    let bucket = BucketGuard::create(client.clone(), false).await;

    let err = client.storage().from(&bucket.name)
        .download("does-not-exist.txt")
        .await
        .expect_err("nonexistent download should fail");
    match err {
        SupabaseError::Storage(_) | SupabaseError::NotFound { .. } => {}
        other => panic!("expected Storage / NotFound error, got {other:?}"),
    }

    bucket.cleanup().await;
}

#[tokio::test]
async fn upload_to_nonexistent_bucket_errors() {
    dotenv().ok();
    let client = admin_client();
    let err = client.storage().from("definitely-not-real-zzz")
        .upload("x.txt", b"x".to_vec(), UploadOptions::default())
        .await
        .expect_err("upload to missing bucket should fail");
    match err {
        SupabaseError::Storage(_) | SupabaseError::NotFound { .. } => {}
        other => panic!("expected Storage / NotFound error, got {other:?}"),
    }
}

#[tokio::test]
async fn empty_bucket_then_delete() {
    dotenv().ok();
    let client = admin_client();
    let bucket = BucketGuard::create(client.clone(), false).await;

    // Add a file, then call empty_bucket. The API on supabase is eventually
    // consistent on the listing — we poll up to ~2s for delete_bucket to
    // succeed (which requires actually-empty state).
    client.storage().from(&bucket.name)
        .upload("temp.txt", b"x".to_vec(), UploadOptions::default())
        .await
        .unwrap();
    client.storage().empty_bucket(&bucket.name).await.unwrap();

    let mut deleted = false;
    for _ in 0..60 {
        if client.storage().delete_bucket(&bucket.name).await.is_ok() {
            deleted = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    assert!(deleted, "delete_bucket never succeeded after empty_bucket — eventual consistency exceeded 12s budget");

    let err = client.storage().get_bucket(&bucket.name).await;
    assert!(err.is_err(), "deleted bucket should not be fetchable");
}

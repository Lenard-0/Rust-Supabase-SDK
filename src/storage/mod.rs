//! Supabase Storage — buckets and object operations.
//!
//! ```no_run
//! # use rust_supabase_sdk::SupabaseClient;
//! # use rust_supabase_sdk::storage::UploadOptions;
//! # async fn demo(client: SupabaseClient) -> rust_supabase_sdk::Result<()> {
//! client.storage().from("avatars")
//!     .upload(
//!         "user-123/avatar.png",
//!         std::fs::read("avatar.png").unwrap_or_default(),
//!         UploadOptions { content_type: Some("image/png".into()), upsert: true, ..Default::default() },
//!     )
//!     .await?;
//!
//! let public_url = client.storage().from("avatars")
//!     .get_public_url("user-123/avatar.png", Default::default());
//! # let _ = public_url; Ok(()) }
//! ```

mod bucket_api;
pub mod types;

pub use bucket_api::BucketApi;
pub use types::{
    Bucket, CreateBucketOptions, FileObject, ImageFormat, ImageResize, ListOptions,
    PublicUrlOptions, SignedUrlEntry, SignedUploadUrl, SortColumn, SortOrder, TransformOptions,
    UpdateBucketOptions, UploadOptions, UploadResponse,
};

use serde_json::Value;

use crate::error::{Result, SupabaseError};
use crate::universals::{HttpMethod, RequestOptions, Service};
use crate::SupabaseClient;

impl SupabaseClient {
    /// Open the storage namespace.
    pub fn storage(&self) -> Storage {
        Storage { client: self.clone() }
    }
}

/// The `storage` namespace.
#[derive(Debug, Clone)]
pub struct Storage {
    pub(crate) client: SupabaseClient,
}

fn storage_opts() -> RequestOptions {
    RequestOptions {
        service: Some(Service::Storage),
        ..RequestOptions::default()
    }
}

impl Storage {
    /// Open the object API for a single bucket.
    pub fn from(&self, bucket: impl Into<String>) -> BucketApi {
        BucketApi::new(self.client.clone(), bucket.into())
    }

    /// List every bucket the current API key can see.
    pub async fn list_buckets(&self) -> Result<Vec<Bucket>> {
        let value = self
            .client
            .request_with("/storage/v1/bucket", HttpMethod::Get, None, &storage_opts())
            .await?;
        decode_json(value)
    }

    /// Fetch a single bucket by id.
    pub async fn get_bucket(&self, id: &str) -> Result<Bucket> {
        let value = self
            .client
            .request_with(
                &format!("/storage/v1/bucket/{id}"),
                HttpMethod::Get,
                None,
                &storage_opts(),
            )
            .await?;
        decode_json(value)
    }

    /// Create a new bucket. Returns the created bucket's id.
    pub async fn create_bucket(&self, id: &str, options: CreateBucketOptions) -> Result<String> {
        let mut body = serde_json::to_value(&options)
            .map_err(|e| SupabaseError::Unexpected(format!("serialize options: {e}")))?;
        if let Value::Object(map) = &mut body {
            map.insert("id".to_string(), Value::String(id.to_string()));
            map.insert("name".to_string(), Value::String(id.to_string()));
        }
        let value = self
            .client
            .request_with(
                "/storage/v1/bucket",
                HttpMethod::Post,
                Some(body),
                &storage_opts(),
            )
            .await?;
        value
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| SupabaseError::Unexpected(format!("create_bucket response: {value}")))
    }

    /// Update a bucket's visibility / limits.
    pub async fn update_bucket(&self, id: &str, options: UpdateBucketOptions) -> Result<()> {
        let body = serde_json::to_value(&options)
            .map_err(|e| SupabaseError::Unexpected(format!("serialize options: {e}")))?;
        self.client
            .request_with(
                &format!("/storage/v1/bucket/{id}"),
                HttpMethod::Put,
                Some(body),
                &storage_opts(),
            )
            .await?;
        Ok(())
    }

    /// Delete every object in a bucket. The bucket itself remains.
    pub async fn empty_bucket(&self, id: &str) -> Result<()> {
        self.client
            .request_with(
                &format!("/storage/v1/bucket/{id}/empty"),
                HttpMethod::Post,
                None,
                &storage_opts(),
            )
            .await?;
        Ok(())
    }

    /// Delete a bucket. Must be empty first — call [`Storage::empty_bucket`] if needed.
    pub async fn delete_bucket(&self, id: &str) -> Result<()> {
        self.client
            .request_with(
                &format!("/storage/v1/bucket/{id}"),
                HttpMethod::Delete,
                None,
                &storage_opts(),
            )
            .await?;
        Ok(())
    }
}

fn decode_json<T: serde::de::DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value.clone()).map_err(|e| SupabaseError::Decode {
        message: e.to_string(),
        body: value.to_string(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decode_json_success_returns_value() {
        let v: Bucket = decode_json(json!({
            "id": "b", "name": "b", "owner": null, "public": false,
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        }))
        .unwrap();
        assert_eq!(v.id, "b");
    }

    #[test]
    fn decode_json_failure_produces_decode_error() {
        // A value that can't deserialize into Vec<Bucket>.
        let err = decode_json::<Vec<Bucket>>(json!("not an array")).unwrap_err();
        match err {
            SupabaseError::Decode { message, body } => {
                assert!(!message.is_empty(), "message should be populated");
                assert_eq!(body, "\"not an array\"");
            }
            other => panic!("expected Decode error, got {other:?}"),
        }
    }
}

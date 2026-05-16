//! Typed representations of Storage API payloads.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A storage bucket. Returned by `list_buckets` / `get_bucket`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Bucket {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub public: bool,
    #[serde(default)]
    pub file_size_limit: Option<u64>,
    #[serde(default)]
    pub allowed_mime_types: Option<Vec<String>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A storage object listing entry.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileObject {
    pub name: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub bucket_id: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_accessed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub metadata: Value,
}

/// Options accepted by [`Storage::create_bucket`](super::Storage::create_bucket).
#[derive(Debug, Clone, Default, Serialize)]
pub struct CreateBucketOptions {
    #[serde(default)]
    pub public: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_size_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_mime_types: Option<Vec<String>>,
}

/// Options accepted by [`Storage::update_bucket`](super::Storage::update_bucket).
#[derive(Debug, Clone, Default, Serialize)]
pub struct UpdateBucketOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_size_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_mime_types: Option<Vec<String>>,
}

/// Options for [`BucketApi::upload`](super::BucketApi::upload) /
/// [`BucketApi::update`](super::BucketApi::update).
#[derive(Debug, Clone, Default)]
pub struct UploadOptions {
    /// Mime type. Defaults to `application/octet-stream` when `None`.
    pub content_type: Option<String>,
    /// Cache-Control header value (e.g. `"3600"`).
    pub cache_control: Option<String>,
    /// Overwrite an existing object at the same path.
    pub upsert: bool,
}

/// Response from [`BucketApi::upload`](super::BucketApi::upload).
#[derive(Debug, Clone, Deserialize)]
pub struct UploadResponse {
    #[serde(default, alias = "Key")]
    pub key: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}

/// Sort key for [`BucketApi::list`](super::BucketApi::list).
#[derive(Debug, Clone, Copy)]
pub enum SortColumn {
    Name,
    UpdatedAt,
    CreatedAt,
    LastAccessedAt,
}

impl SortColumn {
    fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::UpdatedAt => "updated_at",
            Self::CreatedAt => "created_at",
            Self::LastAccessedAt => "last_accessed_at",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SortOrder {
    Asc,
    Desc,
}

impl SortOrder {
    fn as_str(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

/// Options for [`BucketApi::list`](super::BucketApi::list).
#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub search: Option<String>,
    pub sort_by: Option<(SortColumn, SortOrder)>,
}

impl ListOptions {
    pub(crate) fn into_body(self, prefix: &str) -> Value {
        let mut body = serde_json::json!({ "prefix": prefix });
        if let Some(limit) = self.limit {
            body["limit"] = serde_json::json!(limit);
        }
        if let Some(offset) = self.offset {
            body["offset"] = serde_json::json!(offset);
        }
        if let Some(search) = self.search {
            body["search"] = serde_json::json!(search);
        }
        if let Some((column, order)) = self.sort_by {
            body["sortBy"] = serde_json::json!({
                "column": column.as_str(),
                "order": order.as_str(),
            });
        }
        body
    }
}

/// Image transform options for public/signed URLs.
#[derive(Debug, Clone, Default)]
pub struct TransformOptions {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub resize: Option<ImageResize>,
    pub quality: Option<u32>,
    pub format: Option<ImageFormat>,
}

#[derive(Debug, Clone, Copy)]
pub enum ImageResize {
    Cover,
    Contain,
    Fill,
}

impl ImageResize {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Cover => "cover",
            Self::Contain => "contain",
            Self::Fill => "fill",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ImageFormat {
    Origin,
    Webp,
    Avif,
}

impl ImageFormat {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Origin => "origin",
            Self::Webp => "webp",
            Self::Avif => "avif",
        }
    }
}

/// Options for [`BucketApi::get_public_url`](super::BucketApi::get_public_url) /
/// [`BucketApi::create_signed_url`](super::BucketApi::create_signed_url).
#[derive(Debug, Clone, Default)]
pub struct PublicUrlOptions {
    /// If `Some`, forces the response `Content-Disposition: attachment` with the
    /// given filename (or empty for the default filename).
    pub download: Option<String>,
    pub transform: Option<TransformOptions>,
}

impl PublicUrlOptions {
    pub(crate) fn append_to(&self, params: &mut Vec<(String, String)>) {
        if let Some(name) = &self.download {
            if name.is_empty() {
                params.push(("download".into(), String::new()));
            } else {
                params.push(("download".into(), name.clone()));
            }
        }
        if let Some(t) = &self.transform {
            if let Some(w) = t.width {
                params.push(("width".into(), w.to_string()));
            }
            if let Some(h) = t.height {
                params.push(("height".into(), h.to_string()));
            }
            if let Some(r) = t.resize {
                params.push(("resize".into(), r.as_str().to_string()));
            }
            if let Some(q) = t.quality {
                params.push(("quality".into(), q.to_string()));
            }
            if let Some(f) = t.format {
                params.push(("format".into(), f.as_str().to_string()));
            }
        }
    }
}

/// Response from [`BucketApi::create_signed_url`](super::BucketApi::create_signed_url).
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SignedUrlResponse {
    #[serde(alias = "signedURL")]
    pub signed_url: String,
}

/// One entry from [`BucketApi::create_signed_urls`](super::BucketApi::create_signed_urls).
#[derive(Debug, Clone, Deserialize)]
pub struct SignedUrlEntry {
    pub path: Option<String>,
    pub error: Option<String>,
    /// Already absolute (server returns the full URL).
    #[serde(alias = "signedURL", default)]
    pub signed_url: Option<String>,
}

/// Response from [`BucketApi::create_signed_upload_url`](super::BucketApi::create_signed_upload_url).
#[derive(Debug, Clone, Deserialize)]
pub struct SignedUploadUrl {
    pub url: String,
    pub token: String,
    #[serde(default)]
    pub path: Option<String>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- Bucket ---

    #[test]
    fn bucket_deserializes() {
        let v = json!({
            "id": "b1",
            "name": "avatars",
            "owner": "user-1",
            "public": true,
            "file_size_limit": 10485760,
            "allowed_mime_types": ["image/png", "image/jpeg"],
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-06-01T12:00:00Z"
        });
        let b: Bucket = serde_json::from_value(v).unwrap();
        assert_eq!(b.id, "b1");
        assert_eq!(b.name, "avatars");
        assert!(b.public);
        assert_eq!(b.file_size_limit, Some(10_485_760));
        let mime_types = b.allowed_mime_types.unwrap();
        assert_eq!(mime_types, vec!["image/png", "image/jpeg"]);
    }

    #[test]
    fn bucket_deserializes_optional_fields_absent() {
        let v = json!({
            "id": "b2",
            "name": "private",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        });
        let b: Bucket = serde_json::from_value(v).unwrap();
        assert!(!b.public);
        assert!(b.owner.is_none());
        assert!(b.file_size_limit.is_none());
        assert!(b.allowed_mime_types.is_none());
    }

    // --- CreateBucketOptions serialization ---

    #[test]
    fn create_bucket_options_serializes_all_fields() {
        let opts = CreateBucketOptions {
            public: true,
            file_size_limit: Some(5_000_000),
            allowed_mime_types: Some(vec!["image/png".into()]),
        };
        let v = serde_json::to_value(&opts).unwrap();
        assert_eq!(v["public"], true);
        assert_eq!(v["file_size_limit"], 5_000_000);
        assert_eq!(v["allowed_mime_types"][0], "image/png");
    }

    #[test]
    fn create_bucket_options_skips_none_fields() {
        let opts = CreateBucketOptions::default();
        let v = serde_json::to_value(&opts).unwrap();
        assert_eq!(v["public"], false);
        assert!(v.get("file_size_limit").is_none());
        assert!(v.get("allowed_mime_types").is_none());
    }

    // --- UpdateBucketOptions serialization ---

    #[test]
    fn update_bucket_options_skips_none_fields() {
        let opts = UpdateBucketOptions::default();
        let v = serde_json::to_value(&opts).unwrap();
        assert!(v.get("public").is_none());
        assert!(v.get("file_size_limit").is_none());
        assert!(v.get("allowed_mime_types").is_none());
    }

    #[test]
    fn update_bucket_options_includes_set_fields() {
        let opts = UpdateBucketOptions {
            public: Some(false),
            file_size_limit: Some(1024),
            allowed_mime_types: None,
        };
        let v = serde_json::to_value(&opts).unwrap();
        assert_eq!(v["public"], false);
        assert_eq!(v["file_size_limit"], 1024);
        assert!(v.get("allowed_mime_types").is_none());
    }

    // --- UploadResponse deserialization ---

    #[test]
    fn upload_response_deserializes_lowercase_key() {
        let v = json!({ "key": "avatars/user/img.png", "id": "abc", "path": "user/img.png" });
        let r: UploadResponse = serde_json::from_value(v).unwrap();
        assert_eq!(r.key.as_deref(), Some("avatars/user/img.png"));
        assert_eq!(r.id.as_deref(), Some("abc"));
    }

    #[test]
    fn upload_response_deserializes_aliased_key() {
        // Storage may return "Key" (capital K)
        let v = json!({ "Key": "avatars/photo.jpg" });
        let r: UploadResponse = serde_json::from_value(v).unwrap();
        assert_eq!(r.key.as_deref(), Some("avatars/photo.jpg"));
    }

    #[test]
    fn upload_response_all_optional_absent() {
        let v = json!({});
        let r: UploadResponse = serde_json::from_value(v).unwrap();
        assert!(r.key.is_none());
        assert!(r.id.is_none());
        assert!(r.path.is_none());
    }

    // --- SortColumn / SortOrder ---

    #[test]
    fn sort_column_as_str() {
        assert_eq!(SortColumn::Name.as_str(), "name");
        assert_eq!(SortColumn::UpdatedAt.as_str(), "updated_at");
        assert_eq!(SortColumn::CreatedAt.as_str(), "created_at");
        assert_eq!(SortColumn::LastAccessedAt.as_str(), "last_accessed_at");
    }

    #[test]
    fn sort_order_as_str() {
        assert_eq!(SortOrder::Asc.as_str(), "asc");
        assert_eq!(SortOrder::Desc.as_str(), "desc");
    }

    // --- ListOptions::into_body ---

    #[test]
    fn list_options_into_body_minimal() {
        let body = ListOptions::default().into_body("images/");
        assert_eq!(body["prefix"], "images/");
        assert!(body.get("limit").is_none());
        assert!(body.get("offset").is_none());
        assert!(body.get("search").is_none());
        assert!(body.get("sortBy").is_none());
    }

    #[test]
    fn list_options_into_body_all_fields() {
        let body = ListOptions {
            limit: Some(10),
            offset: Some(20),
            search: Some("cat".into()),
            sort_by: Some((SortColumn::Name, SortOrder::Asc)),
        }
        .into_body("docs/");
        assert_eq!(body["prefix"], "docs/");
        assert_eq!(body["limit"], 10);
        assert_eq!(body["offset"], 20);
        assert_eq!(body["search"], "cat");
        assert_eq!(body["sortBy"]["column"], "name");
        assert_eq!(body["sortBy"]["order"], "asc");
    }

    #[test]
    fn list_options_into_body_empty_prefix() {
        let body = ListOptions::default().into_body("");
        assert_eq!(body["prefix"], "");
    }

    // --- ImageResize / ImageFormat ---

    #[test]
    fn image_resize_as_str() {
        assert_eq!(ImageResize::Cover.as_str(), "cover");
        assert_eq!(ImageResize::Contain.as_str(), "contain");
        assert_eq!(ImageResize::Fill.as_str(), "fill");
    }

    #[test]
    fn image_format_as_str() {
        assert_eq!(ImageFormat::Origin.as_str(), "origin");
        assert_eq!(ImageFormat::Webp.as_str(), "webp");
        assert_eq!(ImageFormat::Avif.as_str(), "avif");
    }

    // --- PublicUrlOptions::append_to ---

    #[test]
    fn public_url_options_empty_appends_nothing() {
        let mut params: Vec<(String, String)> = Vec::new();
        PublicUrlOptions::default().append_to(&mut params);
        assert!(params.is_empty());
    }

    #[test]
    fn public_url_options_download_empty_string() {
        let mut params: Vec<(String, String)> = Vec::new();
        PublicUrlOptions {
            download: Some(String::new()),
            ..Default::default()
        }
        .append_to(&mut params);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].0, "download");
        assert_eq!(params[0].1, "");
    }

    #[test]
    fn public_url_options_download_with_name() {
        let mut params: Vec<(String, String)> = Vec::new();
        PublicUrlOptions {
            download: Some("report.pdf".into()),
            ..Default::default()
        }
        .append_to(&mut params);
        assert_eq!(params[0], ("download".into(), "report.pdf".into()));
    }

    #[test]
    fn public_url_options_transform_appended_in_order() {
        let mut params: Vec<(String, String)> = Vec::new();
        PublicUrlOptions {
            transform: Some(TransformOptions {
                width: Some(100),
                height: Some(200),
                resize: Some(ImageResize::Contain),
                quality: Some(75),
                format: Some(ImageFormat::Avif),
            }),
            ..Default::default()
        }
        .append_to(&mut params);
        let map: std::collections::HashMap<_, _> = params.into_iter().collect();
        assert_eq!(map["width"], "100");
        assert_eq!(map["height"], "200");
        assert_eq!(map["resize"], "contain");
        assert_eq!(map["quality"], "75");
        assert_eq!(map["format"], "avif");
    }

    #[test]
    fn public_url_options_partial_transform() {
        let mut params: Vec<(String, String)> = Vec::new();
        PublicUrlOptions {
            transform: Some(TransformOptions {
                width: Some(50),
                ..Default::default()
            }),
            ..Default::default()
        }
        .append_to(&mut params);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], ("width".into(), "50".into()));
    }

    // --- SignedUrlEntry deserialization ---

    #[test]
    fn signed_url_entry_deserializes() {
        let v = json!({
            "path": "photos/cat.jpg",
            "signedURL": "/storage/v1/object/sign/bucket/photos/cat.jpg?token=abc",
            "error": null
        });
        let e: SignedUrlEntry = serde_json::from_value(v).unwrap();
        assert_eq!(e.path.as_deref(), Some("photos/cat.jpg"));
        assert!(e.signed_url.is_some());
        assert!(e.error.is_none());
    }

    #[test]
    fn signed_url_entry_with_error() {
        let v = json!({ "path": "missing.png", "error": "Not found", "signedURL": null });
        let e: SignedUrlEntry = serde_json::from_value(v).unwrap();
        assert_eq!(e.error.as_deref(), Some("Not found"));
        assert!(e.signed_url.is_none());
    }

    // --- SignedUploadUrl deserialization ---

    #[test]
    fn signed_upload_url_deserializes() {
        let v = json!({
            "url": "https://example.supabase.co/storage/v1/object/upload/sign/bucket/file.txt?token=tok",
            "token": "tok",
            "path": "file.txt"
        });
        let s: SignedUploadUrl = serde_json::from_value(v).unwrap();
        assert!(s.url.contains("token=tok"));
        assert_eq!(s.token, "tok");
        assert_eq!(s.path.as_deref(), Some("file.txt"));
    }

    #[test]
    fn signed_upload_url_path_optional() {
        let v = json!({ "url": "https://example.com/sign", "token": "t" });
        let s: SignedUploadUrl = serde_json::from_value(v).unwrap();
        assert!(s.path.is_none());
    }
}

//! Object operations under a single bucket — `storage.from("bucket")`.

use serde_json::{json, Value};

use crate::error::{Result, SupabaseError};
use crate::universals::{decode_error, HttpMethod, RequestOptions, Service};
use crate::SupabaseClient;

use super::types::{
    FileObject, ListOptions, PublicUrlOptions, SignedUploadUrl, SignedUrlEntry,
    SignedUrlResponse, UploadOptions, UploadResponse,
};

/// Object-level API for a single bucket.
#[derive(Debug, Clone)]
pub struct BucketApi {
    pub(crate) client: SupabaseClient,
    pub(crate) bucket: String,
}

fn storage_opts() -> RequestOptions {
    RequestOptions {
        service: Some(Service::Storage),
        ..RequestOptions::default()
    }
}

fn encode_path(path: &str) -> String {
    path.split('/')
        .map(|seg| urlencoding::encode(seg).into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

impl BucketApi {
    pub(crate) fn new(client: SupabaseClient, bucket: String) -> Self {
        Self { client, bucket }
    }

    fn object_path(&self, path: &str) -> String {
        format!(
            "/storage/v1/object/{}/{}",
            urlencoding::encode(&self.bucket),
            encode_path(path)
        )
    }

    /// Upload an object. Fails if the object exists unless `options.upsert` is `true`.
    pub async fn upload(
        &self,
        path: &str,
        body: Vec<u8>,
        options: UploadOptions,
    ) -> Result<UploadResponse> {
        self.upload_inner(path, body, options, HttpMethod::Post).await
    }

    /// Replace an existing object. Equivalent to `upload(.., { upsert: true })` plus
    /// the `PUT` verb, which signals replacement to the storage service.
    pub async fn update(
        &self,
        path: &str,
        body: Vec<u8>,
        options: UploadOptions,
    ) -> Result<UploadResponse> {
        self.upload_inner(path, body, options, HttpMethod::Put).await
    }

    async fn upload_inner(
        &self,
        path: &str,
        body: Vec<u8>,
        options: UploadOptions,
        method: HttpMethod,
    ) -> Result<UploadResponse> {
        let content_type = options
            .content_type
            .unwrap_or_else(|| "application/octet-stream".to_string());

        let mut headers: Vec<(String, String)> = Vec::new();
        if options.upsert {
            headers.push(("x-upsert".into(), "true".into()));
        }
        if let Some(cc) = options.cache_control {
            headers.push(("cache-control".into(), format!("max-age={cc}")));
        }

        let opts = RequestOptions {
            service: Some(Service::Storage),
            headers,
            ..RequestOptions::default()
        };

        let value = self
            .client
            .request_bytes(&self.object_path(path), method, body, &content_type, &opts)
            .await?;

        decode_json::<UploadResponse>(value)
    }

    /// Upload to a pre-signed URL generated via [`create_signed_upload_url`].
    ///
    /// This call does not use the project anon key — the signed `token` carries
    /// the authorization. The bucket and path are encoded in the URL the storage
    /// service returned.
    ///
    /// [`create_signed_upload_url`]: BucketApi::create_signed_upload_url
    pub async fn upload_to_signed_url(
        &self,
        path: &str,
        token: &str,
        body: Vec<u8>,
        options: UploadOptions,
    ) -> Result<UploadResponse> {
        let content_type = options
            .content_type
            .unwrap_or_else(|| "application/octet-stream".to_string());

        let url = format!(
            "{}/storage/v1/object/upload/sign/{}/{}?token={}",
            self.client.url,
            urlencoding::encode(&self.bucket),
            encode_path(path),
            urlencoding::encode(token),
        );

        let mut req = self
            .client
            .http
            .put(&url)
            .header("Content-Type", &content_type)
            .body(body);
        if options.upsert {
            req = req.header("x-upsert", "true");
        }
        let resp = req.send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(decode_error(Service::Storage, status, &text));
        }
        if text.is_empty() {
            return Ok(UploadResponse { key: None, id: None, path: None });
        }
        serde_json::from_str(&text).map_err(|e| SupabaseError::Decode {
            message: e.to_string(),
            body: text,
        })
    }

    /// Download an object's bytes.
    pub async fn download(&self, path: &str) -> Result<Vec<u8>> {
        let resp = self
            .client
            .request_streaming(&self.object_path(path), HttpMethod::Get, &storage_opts())
            .await?;
        Ok(resp.bytes().await?.to_vec())
    }

    /// Download an object as a streaming [`reqwest::Response`]. Use this for
    /// large files — call `Response::bytes_stream` (requires the `stream`
    /// feature on `reqwest`) for chunked access.
    pub async fn download_response(&self, path: &str) -> Result<reqwest::Response> {
        self.client
            .request_streaming(&self.object_path(path), HttpMethod::Get, &storage_opts())
            .await
    }

    /// List objects under `prefix` (use `""` for the bucket root).
    pub async fn list(&self, prefix: &str, options: ListOptions) -> Result<Vec<FileObject>> {
        let body = options.into_body(prefix);
        let value = self
            .client
            .request_with(
                &format!(
                    "/storage/v1/object/list/{}",
                    urlencoding::encode(&self.bucket)
                ),
                HttpMethod::Post,
                Some(body),
                &storage_opts(),
            )
            .await?;
        decode_json::<Vec<FileObject>>(value)
    }

    /// Move an object within the same bucket.
    pub async fn move_(&self, from: &str, to: &str) -> Result<()> {
        let body = json!({
            "bucketId": self.bucket,
            "sourceKey": from,
            "destinationKey": to,
        });
        self.client
            .request_with(
                "/storage/v1/object/move",
                HttpMethod::Post,
                Some(body),
                &storage_opts(),
            )
            .await?;
        Ok(())
    }

    /// Copy an object within the same bucket.
    pub async fn copy(&self, from: &str, to: &str) -> Result<()> {
        let body = json!({
            "bucketId": self.bucket,
            "sourceKey": from,
            "destinationKey": to,
        });
        self.client
            .request_with(
                "/storage/v1/object/copy",
                HttpMethod::Post,
                Some(body),
                &storage_opts(),
            )
            .await?;
        Ok(())
    }

    /// Remove one or more objects. Returns the deleted file records.
    pub async fn remove<I, S>(&self, paths: I) -> Result<Vec<FileObject>>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let prefixes: Vec<String> = paths.into_iter().map(Into::into).collect();
        let body = json!({ "prefixes": prefixes });
        let value = self
            .client
            .request_with(
                &format!("/storage/v1/object/{}", urlencoding::encode(&self.bucket)),
                HttpMethod::Delete,
                Some(body),
                &storage_opts(),
            )
            .await?;
        decode_json::<Vec<FileObject>>(value)
    }

    /// Create a short-lived signed URL granting read access to a private object.
    pub async fn create_signed_url(
        &self,
        path: &str,
        expires_in_secs: u64,
        options: PublicUrlOptions,
    ) -> Result<String> {
        let mut body = json!({ "expiresIn": expires_in_secs });
        if let Some(t) = &options.transform {
            let mut transform = serde_json::Map::new();
            if let Some(w) = t.width {
                transform.insert("width".into(), json!(w));
            }
            if let Some(h) = t.height {
                transform.insert("height".into(), json!(h));
            }
            if let Some(r) = t.resize {
                transform.insert("resize".into(), json!(r.as_str()));
            }
            if let Some(q) = t.quality {
                transform.insert("quality".into(), json!(q));
            }
            if let Some(f) = t.format {
                transform.insert("format".into(), json!(f.as_str()));
            }
            body["transform"] = Value::Object(transform);
        }

        let value = self
            .client
            .request_with(
                &format!(
                    "/storage/v1/object/sign/{}/{}",
                    urlencoding::encode(&self.bucket),
                    encode_path(path)
                ),
                HttpMethod::Post,
                Some(body),
                &storage_opts(),
            )
            .await?;
        let resp: SignedUrlResponse = decode_json(value)?;

        let mut absolute = if resp.signed_url.starts_with("http") {
            resp.signed_url
        } else {
            format!("{}{}", self.client.url, resp.signed_url)
        };
        if let Some(dl) = &options.download {
            let sep = if absolute.contains('?') { '&' } else { '?' };
            absolute.push(sep);
            absolute.push_str("download=");
            if !dl.is_empty() {
                absolute.push_str(&urlencoding::encode(dl));
            }
        }
        Ok(absolute)
    }

    /// Create signed URLs for many paths at once.
    pub async fn create_signed_urls<I, S>(
        &self,
        paths: I,
        expires_in_secs: u64,
    ) -> Result<Vec<SignedUrlEntry>>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let paths: Vec<String> = paths.into_iter().map(Into::into).collect();
        let body = json!({ "paths": paths, "expiresIn": expires_in_secs });
        let value = self
            .client
            .request_with(
                &format!("/storage/v1/object/sign/{}", urlencoding::encode(&self.bucket)),
                HttpMethod::Post,
                Some(body),
                &storage_opts(),
            )
            .await?;
        let mut entries: Vec<SignedUrlEntry> = decode_json(value)?;
        for entry in &mut entries {
            if let Some(url) = entry.signed_url.as_mut() {
                if !url.starts_with("http") {
                    *url = format!("{}{}", self.client.url, url);
                }
            }
        }
        Ok(entries)
    }

    /// Create a pre-signed URL that lets a third party upload a single object.
    pub async fn create_signed_upload_url(&self, path: &str) -> Result<SignedUploadUrl> {
        let value = self
            .client
            .request_with(
                &format!(
                    "/storage/v1/object/upload/sign/{}/{}",
                    urlencoding::encode(&self.bucket),
                    encode_path(path)
                ),
                HttpMethod::Post,
                None,
                &storage_opts(),
            )
            .await?;
        let mut signed: SignedUploadUrl = decode_json(value)?;
        if !signed.url.starts_with("http") {
            signed.url = format!("{}{}", self.client.url, signed.url);
        }
        Ok(signed)
    }

    /// Construct the public URL for an object in a public bucket. Does not hit
    /// the network — returns the URL synchronously.
    pub fn get_public_url(&self, path: &str, options: PublicUrlOptions) -> String {
        let base = if options.transform.is_some() {
            format!(
                "{}/storage/v1/render/image/public/{}/{}",
                self.client.url,
                urlencoding::encode(&self.bucket),
                encode_path(path)
            )
        } else {
            format!(
                "{}/storage/v1/object/public/{}/{}",
                self.client.url,
                urlencoding::encode(&self.bucket),
                encode_path(path)
            )
        };

        let mut params: Vec<(String, String)> = Vec::new();
        options.append_to(&mut params);
        if params.is_empty() {
            base
        } else {
            let qs: Vec<String> = params
                .into_iter()
                .map(|(k, v)| {
                    if v.is_empty() {
                        k
                    } else {
                        format!("{}={}", urlencoding::encode(&k), urlencoding::encode(&v))
                    }
                })
                .collect();
            format!("{base}?{}", qs.join("&"))
        }
    }
}

fn decode_json<T: serde::de::DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value.clone()).map_err(|e| SupabaseError::Decode {
        message: e.to_string(),
        body: value.to_string(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::storage::types::{FileObject, ImageFormat, ImageResize, TransformOptions};
    use crate::SupabaseClient;

    fn api() -> BucketApi {
        SupabaseClient::new("https://example.supabase.co", "anon", None)
            .storage()
            .from("avatars")
    }

    #[test]
    fn public_url_no_options() {
        let url = api().get_public_url("user/avatar.png", Default::default());
        assert_eq!(
            url,
            "https://example.supabase.co/storage/v1/object/public/avatars/user/avatar.png"
        );
    }

    #[test]
    fn public_url_with_download() {
        let url = api().get_public_url(
            "user/avatar.png",
            PublicUrlOptions {
                download: Some("my-avatar.png".into()),
                ..Default::default()
            },
        );
        assert!(url.contains("?download=my-avatar.png"));
    }

    #[test]
    fn public_url_with_transform_routes_through_render() {
        let url = api().get_public_url(
            "user/avatar.png",
            PublicUrlOptions {
                transform: Some(TransformOptions {
                    width: Some(200),
                    height: Some(200),
                    resize: Some(ImageResize::Cover),
                    quality: Some(80),
                    format: Some(ImageFormat::Webp),
                }),
                ..Default::default()
            },
        );
        assert!(url.contains("/storage/v1/render/image/public/avatars/user/avatar.png"));
        assert!(url.contains("width=200"));
        assert!(url.contains("height=200"));
        assert!(url.contains("resize=cover"));
        assert!(url.contains("quality=80"));
        assert!(url.contains("format=webp"));
    }

    #[test]
    fn object_path_encodes_segments() {
        let p = api().object_path("folder with spaces/file & punc.png");
        assert!(p.contains("folder%20with%20spaces"));
        assert!(p.contains("file%20%26%20punc.png"));
        // Slashes between segments must remain literal.
        assert!(p.contains("/folder%20with%20spaces/file"));
    }

    #[test]
    fn file_object_deserializes_real_payload() {
        let v = serde_json::json!({
            "name": "avatar.png",
            "id": "1234-abcd",
            "bucket_id": "avatars",
            "owner": "user-1",
            "updated_at": "2024-05-01T12:00:00Z",
            "created_at": "2024-05-01T11:00:00Z",
            "last_accessed_at": "2024-05-02T09:00:00Z",
            "metadata": { "size": 4321, "mimetype": "image/png" }
        });
        let f: FileObject = serde_json::from_value(v).unwrap();
        assert_eq!(f.name, "avatar.png");
        assert_eq!(f.bucket_id.as_deref(), Some("avatars"));
        assert_eq!(f.metadata["size"], 4321);
    }

    #[test]
    fn list_options_body_omits_unset_fields() {
        use crate::storage::types::{SortColumn, SortOrder};
        let body = ListOptions {
            limit: Some(50),
            sort_by: Some((SortColumn::CreatedAt, SortOrder::Desc)),
            ..Default::default()
        }
        .into_body("avatars/");
        assert_eq!(body["prefix"], "avatars/");
        assert_eq!(body["limit"], 50);
        assert_eq!(body["sortBy"]["column"], "created_at");
        assert_eq!(body["sortBy"]["order"], "desc");
        assert!(body.get("offset").is_none());
        assert!(body.get("search").is_none());
    }

    // --- encode_path ---

    #[test]
    fn encode_path_passthrough_plain_segments() {
        assert_eq!(encode_path("folder/file.png"), "folder/file.png");
    }

    #[test]
    fn encode_path_encodes_spaces_in_each_segment() {
        assert_eq!(encode_path("my folder/my file.png"), "my%20folder/my%20file.png");
    }

    #[test]
    fn encode_path_encodes_special_chars() {
        assert_eq!(encode_path("a+b/c&d"), "a%2Bb/c%26d");
    }

    #[test]
    fn encode_path_preserves_slash_boundaries() {
        let result = encode_path("a b/c d/e f");
        let parts: Vec<&str> = result.split('/').collect();
        assert_eq!(parts, ["a%20b", "c%20d", "e%20f"]);
    }

    // --- get_public_url edge cases ---

    #[test]
    fn public_url_bucket_with_special_chars() {
        let api = SupabaseClient::new("https://proj.supabase.co", "anon", None)
            .storage()
            .from("my bucket");
        let url = api.get_public_url("file.png", Default::default());
        assert!(url.contains("my%20bucket"), "bucket name should be percent-encoded: {url}");
    }

    #[test]
    fn public_url_deeply_nested_path() {
        let url = api().get_public_url("a/b/c/d.png", Default::default());
        assert!(url.ends_with("/avatars/a/b/c/d.png"), "{url}");
    }

    #[test]
    fn public_url_empty_download_adds_bare_flag() {
        let url = api().get_public_url(
            "file.bin",
            PublicUrlOptions {
                download: Some(String::new()),
                ..Default::default()
            },
        );
        assert!(url.contains("?download"), "{url}");
        // Value after `=` should be absent: `?download` not `?download=something`
        let qs = url.split('?').nth(1).unwrap_or("");
        assert!(qs.starts_with("download"), "{qs}");
    }

    #[test]
    fn public_url_transform_width_only() {
        let url = api().get_public_url(
            "photo.jpg",
            PublicUrlOptions {
                transform: Some(TransformOptions {
                    width: Some(320),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        assert!(url.contains("/render/image/public/"), "{url}");
        assert!(url.contains("width=320"), "{url}");
        assert!(!url.contains("height="), "{url}");
    }

    #[test]
    fn public_url_transform_all_params_present() {
        let url = api().get_public_url(
            "img.jpg",
            PublicUrlOptions {
                transform: Some(TransformOptions {
                    width: Some(100),
                    height: Some(200),
                    resize: Some(ImageResize::Fill),
                    quality: Some(60),
                    format: Some(ImageFormat::Avif),
                }),
                ..Default::default()
            },
        );
        assert!(url.contains("width=100"));
        assert!(url.contains("height=200"));
        assert!(url.contains("resize=fill"));
        assert!(url.contains("quality=60"));
        assert!(url.contains("format=avif"));
    }

    // --- decode_json ---

    #[test]
    fn decode_json_succeeds_on_valid_value() {
        let v = serde_json::json!({"key": "bucket/file.png", "id": "123"});
        let r: UploadResponse = decode_json(v).unwrap();
        assert_eq!(r.key.as_deref(), Some("bucket/file.png"));
    }

    #[test]
    fn decode_json_returns_error_on_bad_type() {
        let v = serde_json::json!("not an object");
        let err = decode_json::<UploadResponse>(v).unwrap_err();
        // Should be a Decode error.
        assert!(
            matches!(err, crate::error::SupabaseError::Decode { .. }),
            "Expected Decode variant, got {err:?}"
        );
    }

    // --- object_path ---

    #[test]
    fn object_path_includes_bucket_and_file() {
        let path = api().object_path("profile/pic.jpg");
        assert_eq!(path, "/storage/v1/object/avatars/profile/pic.jpg");
    }

    #[test]
    fn object_path_encodes_bucket_name() {
        let api = SupabaseClient::new("https://proj.supabase.co", "anon", None)
            .storage()
            .from("user files");
        let path = api.object_path("test.png");
        assert!(path.contains("user%20files"), "{path}");
    }
}

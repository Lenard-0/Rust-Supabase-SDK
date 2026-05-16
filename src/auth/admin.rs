//! Service-role admin operations. Requires the project's `service_role` key
//! as the client's `api_key`.

use serde_json::{json, Value};

use crate::error::{Result, SupabaseError};
use crate::universals::{HttpMethod, RequestOptions};
use crate::SupabaseClient;

use super::types::{AdminUserAttributes, OtpType, User};

#[derive(Debug, Clone)]
pub struct AuthAdmin {
    pub(crate) client: SupabaseClient,
}

/// Result of [`AuthAdmin::list_users`].
#[derive(Debug, Clone)]
pub struct ListUsersPage {
    pub users: Vec<User>,
    /// Total number of users in the project (from the `X-Total-Count` header,
    /// or `None` if the server didn't include it).
    pub total: Option<u64>,
    /// The next page index reported by the server, when applicable.
    pub next_page: Option<u32>,
}

/// Response payload from [`AuthAdmin::generate_link`].
#[derive(Debug, Clone, serde::Deserialize)]
pub struct GenerateLinkResponse {
    #[serde(default)]
    pub action_link: Option<String>,
    #[serde(default)]
    pub email_otp: Option<String>,
    #[serde(default)]
    pub hashed_token: Option<String>,
    #[serde(default)]
    pub verification_type: Option<String>,
    #[serde(default)]
    pub redirect_to: Option<String>,
    #[serde(flatten)]
    pub user: User,
}

impl AuthAdmin {
    pub(crate) fn new(client: SupabaseClient) -> Self {
        Self { client }
    }

    /// List users with pagination.
    pub async fn list_users(&self, page: u32, per_page: u32) -> Result<ListUsersPage> {
        let path = format!("/auth/v1/admin/users?page={page}&per_page={per_page}");
        let (_status, headers, body) = self
            .client
            .request_full(&path, HttpMethod::Get, None, &RequestOptions::auth())
            .await?;

        let value: Value = if body.is_empty() {
            json!({ "users": [] })
        } else {
            serde_json::from_str(&body).map_err(|e| SupabaseError::Decode {
                message: e.to_string(),
                body: body.clone(),
            })?
        };

        let users_value = value
            .get("users")
            .cloned()
            .ok_or_else(|| SupabaseError::Unexpected("Missing `users` in response".into()))?;
        let users: Vec<User> =
            serde_json::from_value(users_value).map_err(|e| SupabaseError::Decode {
                message: e.to_string(),
                body: body.clone(),
            })?;

        let total = headers
            .get("x-total-count")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());

        let next_page = headers
            .get("link")
            .and_then(|v| v.to_str().ok())
            .and_then(parse_next_page);

        Ok(ListUsersPage { users, total, next_page })
    }

    /// Fetch a user by their `id`.
    pub async fn get_user_by_id(&self, user_id: &str) -> Result<User> {
        let value = self
            .client
            .request_with(
                &format!("/auth/v1/admin/users/{user_id}"),
                HttpMethod::Get,
                None,
                &RequestOptions::auth(),
            )
            .await?;
        decode_user(value)
    }

    /// Create a user with admin privileges (skips confirmation flows when the
    /// `email_confirm` / `phone_confirm` flags are set).
    pub async fn create_user(&self, attrs: AdminUserAttributes) -> Result<User> {
        let body = serde_json::to_value(&attrs)
            .map_err(|e| SupabaseError::Unexpected(format!("serialize attrs: {e}")))?;
        let value = self
            .client
            .request_with(
                "/auth/v1/admin/users",
                HttpMethod::Post,
                Some(body),
                &RequestOptions::auth(),
            )
            .await?;
        decode_user(value)
    }

    /// Update a user by id.
    pub async fn update_user_by_id(
        &self,
        user_id: &str,
        attrs: AdminUserAttributes,
    ) -> Result<User> {
        let body = serde_json::to_value(&attrs)
            .map_err(|e| SupabaseError::Unexpected(format!("serialize attrs: {e}")))?;
        let value = self
            .client
            .request_with(
                &format!("/auth/v1/admin/users/{user_id}"),
                HttpMethod::Put,
                Some(body),
                &RequestOptions::auth(),
            )
            .await?;
        decode_user(value)
    }

    /// Delete (or soft-delete) a user.
    pub async fn delete_user(&self, user_id: &str, soft_delete: bool) -> Result<()> {
        let path = format!("/auth/v1/admin/users/{user_id}");
        let body = if soft_delete {
            Some(json!({ "should_soft_delete": true }))
        } else {
            None
        };
        self.client
            .request_with(&path, HttpMethod::Delete, body, &RequestOptions::auth())
            .await?;
        Ok(())
    }

    /// Invite a new user by email.
    pub async fn invite_user_by_email(
        &self,
        email: &str,
        redirect_to: Option<&str>,
        user_metadata: Option<Value>,
    ) -> Result<User> {
        let mut body = json!({ "email": email });
        if let Some(redirect) = redirect_to {
            body["redirect_to"] = json!(redirect);
        }
        if let Some(meta) = user_metadata {
            body["data"] = meta;
        }
        let value = self
            .client
            .request_with(
                "/auth/v1/admin/invite",
                HttpMethod::Post,
                Some(body),
                &RequestOptions::auth(),
            )
            .await?;
        decode_user(value)
    }

    /// Generate an action link (signup confirmation, recovery, magic-link, etc.)
    /// for a given user. Useful for sending the link through a custom channel.
    pub async fn generate_link(
        &self,
        link_type: OtpType,
        email: &str,
        password: Option<&str>,
        new_email: Option<&str>,
        redirect_to: Option<&str>,
        user_metadata: Option<Value>,
    ) -> Result<GenerateLinkResponse> {
        let mut body = json!({ "type": link_type.as_str(), "email": email });
        if let Some(p) = password {
            body["password"] = json!(p);
        }
        if let Some(ne) = new_email {
            body["new_email"] = json!(ne);
        }
        if let Some(rd) = redirect_to {
            body["redirect_to"] = json!(rd);
        }
        if let Some(meta) = user_metadata {
            body["data"] = meta;
        }
        let value = self
            .client
            .request_with(
                "/auth/v1/admin/generate_link",
                HttpMethod::Post,
                Some(body),
                &RequestOptions::auth(),
            )
            .await?;
        serde_json::from_value(value.clone()).map_err(|e| SupabaseError::Decode {
            message: e.to_string(),
            body: value.to_string(),
        })
    }
}

fn decode_user(value: Value) -> Result<User> {
    serde_json::from_value(value.clone()).map_err(|e| SupabaseError::Decode {
        message: e.to_string(),
        body: value.to_string(),
    })
}

/// Parse a `Link` header to extract the `?page=N` from a `rel="next"` entry.
fn parse_next_page(link: &str) -> Option<u32> {
    for part in link.split(',') {
        let part = part.trim();
        if part.contains("rel=\"next\"") {
            let url_part = part.split(';').next()?.trim().trim_matches(|c| c == '<' || c == '>');
            return url_part
                .split('?')
                .nth(1)?
                .split('&')
                .find_map(|kv| kv.strip_prefix("page="))
                .and_then(|n| n.parse::<u32>().ok());
        }
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_next_page_extracts_page_param() {
        let link = r#"<https://x.co/users?page=3&per_page=20>; rel="next""#;
        assert_eq!(parse_next_page(link), Some(3));
    }

    #[test]
    fn parse_next_page_with_multiple_rels_picks_next() {
        let link = r#"<https://x.co?page=1>; rel="prev", <https://x.co?page=5>; rel="next""#;
        assert_eq!(parse_next_page(link), Some(5));
    }

    #[test]
    fn parse_next_page_missing_next_rel_returns_none() {
        let link = r#"<https://x.co?page=1>; rel="prev""#;
        assert_eq!(parse_next_page(link), None);
    }

    #[test]
    fn parse_next_page_missing_page_param_returns_none() {
        let link = r#"<https://x.co?per_page=20>; rel="next""#;
        assert_eq!(parse_next_page(link), None);
    }

    #[test]
    fn parse_next_page_empty_string_returns_none() {
        assert_eq!(parse_next_page(""), None);
    }

    #[test]
    fn parse_next_page_unparseable_page_returns_none() {
        let link = r#"<https://x.co?page=not-a-number>; rel="next""#;
        assert_eq!(parse_next_page(link), None);
    }

    #[test]
    fn decode_user_success() {
        let v = json!({
            "id": "u1", "aud": "auth", "role": "auth",
            "created_at": "2024-01-01T00:00:00Z"
        });
        let u = decode_user(v).unwrap();
        assert_eq!(u.id, "u1");
    }

    #[test]
    fn decode_user_failure_returns_decode_error() {
        // Missing required `id` field.
        let v = json!({"aud": "auth"});
        let err = decode_user(v).unwrap_err();
        assert!(matches!(err, SupabaseError::Decode { .. }));
    }

    #[test]
    fn list_users_page_struct_shape() {
        let p = ListUsersPage {
            users: Vec::new(),
            total: Some(0),
            next_page: None,
        };
        assert!(p.users.is_empty());
        assert_eq!(p.total, Some(0));
        assert!(p.next_page.is_none());
    }
}

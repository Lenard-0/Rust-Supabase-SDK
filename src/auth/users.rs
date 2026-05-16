//! Legacy admin user helpers — preserved as deprecated wrappers.
//!
//! New code should use [`Auth::admin()`](super::Auth::admin) which returns an
//! [`AuthAdmin`](super::AuthAdmin) view with paginated, fully-typed methods.

use serde::Deserialize;

use crate::error::{Result, SupabaseError};
use crate::universals::{HttpMethod, RequestOptions};
use crate::SupabaseClient;

/// **Deprecated:** use [`User`](crate::User), which exposes the full GoTrue payload
/// (identities, app_metadata, user_metadata, etc.).
#[deprecated(since = "0.5.0", note = "use `rust_supabase_sdk::User`")]
#[derive(Debug, Clone, Deserialize)]
pub struct SupabaseUser {
    pub id: String,
    pub email: String,
    #[serde(default)]
    pub name: Option<String>,
    pub created_at: String,
}

impl SupabaseClient {
    /// **Deprecated:** use [`client.auth().admin().list_users(page, per_page)`](super::AuthAdmin::list_users).
    #[deprecated(since = "0.5.0", note = "use `client.auth().admin().list_users(page, per_page)`")]
    #[allow(deprecated)]
    pub async fn get_all_users(&self) -> Result<Vec<SupabaseUser>> {
        let response = self
            .request_with(
                "/auth/v1/admin/users",
                HttpMethod::Get,
                None,
                &RequestOptions::auth(),
            )
            .await?;
        let users_value = response
            .get("users")
            .cloned()
            .ok_or_else(|| SupabaseError::Unexpected("Missing `users` field in response".into()))?;
        serde_json::from_value(users_value).map_err(|e| SupabaseError::Decode {
            message: format!("Failed to deserialize users: {e}"),
            body: response.to_string(),
        })
    }

    /// **Deprecated:** use [`client.auth().admin().get_user_by_id(id)`](super::AuthAdmin::get_user_by_id).
    #[deprecated(since = "0.5.0", note = "use `client.auth().admin().get_user_by_id(user_id)`")]
    #[allow(deprecated)]
    pub async fn get_user_by_id(&self, user_id: &str) -> Result<SupabaseUser> {
        let response = self
            .request_with(
                &format!("/auth/v1/admin/users/{user_id}"),
                HttpMethod::Get,
                None,
                &RequestOptions::auth(),
            )
            .await?;
        serde_json::from_value(response.clone()).map_err(|e| SupabaseError::Decode {
            message: format!("Failed to deserialize user: {e}"),
            body: response.to_string(),
        })
    }
}

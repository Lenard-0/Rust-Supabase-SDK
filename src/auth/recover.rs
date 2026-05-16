//! Legacy password-recovery helpers — preserved as deprecated wrappers around
//! the new auth namespace.

use serde_json::json;

use crate::error::Result;
use crate::universals::{HttpMethod, RequestOptions};
use crate::SupabaseClient;

impl SupabaseClient {
    /// **Deprecated:** use [`client.auth().reset_password_for_email(...)`](super::Auth::reset_password_for_email).
    #[deprecated(
        since = "0.5.0",
        note = "use `client.auth().reset_password_for_email(email, ResetPasswordOptions::default())`"
    )]
    pub async fn forgot_password(&self, email: &str) -> Result<()> {
        self.request_with(
            "/auth/v1/recover",
            HttpMethod::Post,
            Some(json!({ "email": email })),
            &RequestOptions::auth(),
        )
        .await?;
        Ok(())
    }

    /// **Deprecated:** combines recovery-OTP exchange with `update_user`. Prefer
    /// the new flow: call [`Auth::verify_otp`](super::Auth::verify_otp) with
    /// [`OtpType::Recovery`](super::OtpType::Recovery), then
    /// [`Auth::update_user`](super::Auth::update_user) with the new password.
    #[deprecated(
        since = "0.5.0",
        note = "use `client.auth().verify_otp(...)` then `client.auth().update_user(...)`"
    )]
    pub async fn reset_password(
        &self,
        new_password: &str,
        access_token: &str,
        otp: &str,
    ) -> Result<()> {
        let opts = RequestOptions {
            bearer_override: Some(access_token.to_string()),
            ..RequestOptions::auth()
        };
        self.request_with(
            "/auth/v1/user",
            HttpMethod::Put,
            Some(json!({ "password": new_password, "code": otp })),
            &opts,
        )
        .await?;
        Ok(())
    }
}

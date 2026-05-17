//! Authentication & user management — modeled on `supabase-js`'s `auth` namespace.
//!
//! ```no_run
//! # use rust_supabase_sdk::SupabaseClient;
//! # async fn demo(client: SupabaseClient) -> rust_supabase_sdk::Result<()> {
//! let session = client
//!     .auth()
//!     .sign_in_with_password("alice@example.com", "hunter2")
//!     .await?;
//! let user = client.auth().get_user().await?;
//! # let _ = (session, user); Ok(())
//! # }
//! ```
//!
//! After a successful sign-in the session is stored on the client's
//! [`SessionStore`](crate::SessionStore), so subsequent PostgREST and Storage
//! requests automatically use the user's access token.

use serde_json::{json, Value};

use crate::error::{AuthError, Result, SupabaseError};
use crate::universals::{HttpMethod, RequestOptions};
use crate::SupabaseClient;

pub mod admin;
pub mod oauth;
pub mod session_store;
pub mod types;

pub use admin::AuthAdmin;
pub use types::{
    AdminUserAttributes, Identity, OAuthFlow, OAuthOptions, OAuthProvider, OtpOptions,
    OtpRecipient, OtpType, ResetPasswordOptions, Session, SignOutScope, SignUpOptions,
    UpdateUserAttributes, User, VerifyOtpParams,
};

impl SupabaseClient {
    /// Open the auth namespace.
    pub fn auth(&self) -> Auth {
        Auth { client: self.clone() }
    }
}

/// The `auth` namespace.
#[derive(Debug, Clone)]
pub struct Auth {
    pub(crate) client: SupabaseClient,
}

impl Auth {
    fn endpoint(&self, path: &str) -> String {
        format!("/auth/v1{path}")
    }

    /// Service-role admin operations.
    pub fn admin(&self) -> AuthAdmin {
        AuthAdmin::new(self.client.clone())
    }

    /// The currently cached session, if any. Does not hit the network.
    pub fn get_session(&self) -> Option<Session> {
        self.client.session_store.get()
    }

    /// Replace the active session.
    pub fn set_session(&self, session: Session) {
        self.client.session_store.set(session);
    }

    /// Clear the cached session locally (no network call). See [`Auth::sign_out`]
    /// to also revoke the session on the server.
    pub fn clear_session(&self) {
        self.client.session_store.clear();
    }

    /// Register a new user. Equivalent to `supabase.auth.signUp`.
    pub async fn sign_up(
        &self,
        email: &str,
        password: &str,
        options: SignUpOptions,
    ) -> Result<Session> {
        let body = build_sign_up_body(email, password, &options);
        let value = self
            .client
            .request_with(
                &self.endpoint("/signup"),
                HttpMethod::Post,
                Some(body),
                &RequestOptions::auth(),
            )
            .await?;

        // GoTrue may return either a session (auto-confirm enabled) or just a User
        // (confirmation email pending). Try both shapes.
        if value.get("access_token").is_some() {
            let session = parse_session(value)?;
            self.client.session_store.set(session.clone());
            Ok(session)
        } else {
            Err(SupabaseError::Auth(AuthError::from_message(
                "Sign-up requires email confirmation — no session was returned",
            )))
        }
    }

    /// Sign in with an email/phone + password.
    pub async fn sign_in_with_password(
        &self,
        email_or_phone: &str,
        password: &str,
    ) -> Result<Session> {
        let body = if email_or_phone.contains('@') {
            json!({ "email": email_or_phone, "password": password })
        } else {
            json!({ "phone": email_or_phone, "password": password })
        };
        let session = self.token_request("password", body).await?;
        self.client.session_store.set(session.clone());
        Ok(session)
    }

    /// Sign in via a one-time password (magic link or SMS code).
    /// This call only *requests* the OTP — the user completes the flow by
    /// calling [`Auth::verify_otp`].
    pub async fn sign_in_with_otp(
        &self,
        recipient: OtpRecipient,
        options: OtpOptions,
    ) -> Result<()> {
        let mut body = match recipient {
            OtpRecipient::Email(e) => json!({ "email": e }),
            OtpRecipient::Phone(p) => json!({ "phone": p }),
        };
        if let Some(should_create) = options.should_create_user {
            body["create_user"] = json!(should_create);
        }
        if let Some(redirect) = options.email_redirect_to {
            body["email_redirect_to"] = json!(redirect);
        }
        if let Some(data) = options.user_metadata {
            body["data"] = data;
        }
        if let Some(captcha) = options.captcha_token {
            body["gotrue_meta_security"] = json!({ "captcha_token": captcha });
        }
        if let Some(channel) = options.channel {
            body["channel"] = json!(channel);
        }
        self.client
            .request_with(
                &self.endpoint("/otp"),
                HttpMethod::Post,
                Some(body),
                &RequestOptions::auth(),
            )
            .await?;
        Ok(())
    }

    /// Verify an OTP / magic-link code and exchange it for a session.
    pub async fn verify_otp(&self, params: VerifyOtpParams) -> Result<Session> {
        let body = match params {
            VerifyOtpParams::Email { email, token, otp_type } => json!({
                "email": email, "token": token, "type": otp_type.as_str()
            }),
            VerifyOtpParams::Phone { phone, token, otp_type } => json!({
                "phone": phone, "token": token, "type": otp_type.as_str()
            }),
            VerifyOtpParams::TokenHash { token_hash, otp_type } => json!({
                "token_hash": token_hash, "type": otp_type.as_str()
            }),
        };
        let value = self
            .client
            .request_with(
                &self.endpoint("/verify"),
                HttpMethod::Post,
                Some(body),
                &RequestOptions::auth(),
            )
            .await?;
        let session = parse_session(value)?;
        self.client.session_store.set(session.clone());
        Ok(session)
    }

    /// Resend the most recent OTP / confirmation. Mirrors GoTrue's `/resend`.
    pub async fn resend(&self, recipient: OtpRecipient, otp_type: OtpType) -> Result<()> {
        let body = match recipient {
            OtpRecipient::Email(e) => json!({ "email": e, "type": otp_type.as_str() }),
            OtpRecipient::Phone(p) => json!({ "phone": p, "type": otp_type.as_str() }),
        };
        self.client
            .request_with(
                &self.endpoint("/resend"),
                HttpMethod::Post,
                Some(body),
                &RequestOptions::auth(),
            )
            .await?;
        Ok(())
    }

    /// Sign in anonymously. Returns a session for a freshly-created anonymous user.
    pub async fn sign_in_anonymously(&self, captcha_token: Option<String>) -> Result<Session> {
        let body = if let Some(token) = captcha_token {
            json!({ "gotrue_meta_security": { "captcha_token": token } })
        } else {
            json!({})
        };
        let value = self
            .client
            .request_with(
                &self.endpoint("/signup"),
                HttpMethod::Post,
                Some(body),
                &RequestOptions::auth(),
            )
            .await?;
        let session = parse_session(value)?;
        self.client.session_store.set(session.clone());
        Ok(session)
    }

    /// Sign in with an ID token from a third-party provider (Google, Apple, etc.).
    pub async fn sign_in_with_id_token(
        &self,
        provider: &str,
        id_token: &str,
        nonce: Option<&str>,
    ) -> Result<Session> {
        let mut body = json!({ "provider": provider, "id_token": id_token });
        if let Some(n) = nonce {
            body["nonce"] = json!(n);
        }
        let session = self.token_request("id_token", body).await?;
        self.client.session_store.set(session.clone());
        Ok(session)
    }

    /// Build the authorization URL for an OAuth flow. The caller is responsible
    /// for directing the user there and (eventually) exchanging the returned
    /// `code` via [`Auth::exchange_code_for_session`].
    pub fn sign_in_with_oauth(
        &self,
        provider: impl Into<String>,
        options: OAuthOptions,
    ) -> OAuthFlow {
        oauth::build_authorize_url(&self.client.url, provider.into(), options)
    }

    /// Exchange a PKCE/OAuth `code` for a session.
    pub async fn exchange_code_for_session(&self, code: &str) -> Result<Session> {
        let body = json!({ "auth_code": code });
        let session = self.token_request("pkce", body).await?;
        self.client.session_store.set(session.clone());
        Ok(session)
    }

    /// Fetch the user behind the current session (or `access_token` override).
    pub async fn get_user(&self) -> Result<User> {
        let session = self.client.session_store.get();
        let opts = match &session {
            Some(s) => RequestOptions {
                bearer_override: Some(s.access_token.clone()),
                ..RequestOptions::auth()
            },
            None => RequestOptions::auth(),
        };
        let value = self
            .client
            .request_with(&self.endpoint("/user"), HttpMethod::Get, None, &opts)
            .await?;
        serde_json::from_value(value.clone()).map_err(|e| SupabaseError::Decode {
            message: e.to_string(),
            body: value.to_string(),
        })
    }

    /// Update the authenticated user (email, phone, password, metadata).
    pub async fn update_user(&self, attrs: UpdateUserAttributes) -> Result<User> {
        let body = serde_json::to_value(&attrs)
            .map_err(|e| SupabaseError::Unexpected(format!("serialize attrs: {e}")))?;
        let session = self.client.session_store.get();
        let opts = match &session {
            Some(s) => RequestOptions {
                bearer_override: Some(s.access_token.clone()),
                ..RequestOptions::auth()
            },
            None => RequestOptions::auth(),
        };
        let value = self
            .client
            .request_with(&self.endpoint("/user"), HttpMethod::Put, Some(body), &opts)
            .await?;
        serde_json::from_value(value.clone()).map_err(|e| SupabaseError::Decode {
            message: e.to_string(),
            body: value.to_string(),
        })
    }

    /// Send a password-recovery email.
    pub async fn reset_password_for_email(
        &self,
        email: &str,
        options: ResetPasswordOptions,
    ) -> Result<()> {
        let mut body = json!({ "email": email });
        if let Some(redirect) = options.redirect_to {
            body["redirect_to"] = json!(redirect);
        }
        if let Some(captcha) = options.captcha_token {
            body["gotrue_meta_security"] = json!({ "captcha_token": captcha });
        }
        self.client
            .request_with(
                &self.endpoint("/recover"),
                HttpMethod::Post,
                Some(body),
                &RequestOptions::auth(),
            )
            .await?;
        Ok(())
    }

    /// Refresh the access token using a refresh token. Defaults to the
    /// currently stored session's refresh token when `refresh_token` is `None`.
    pub async fn refresh_session(&self, refresh_token: Option<&str>) -> Result<Session> {
        let token = match refresh_token {
            Some(t) => t.to_string(),
            None => self
                .client
                .session_store
                .get()
                .map(|s| s.refresh_token)
                .ok_or_else(|| {
                    SupabaseError::Auth(AuthError::from_message(
                        "No refresh token available — call sign_in_with_password first",
                    ))
                })?,
        };
        let session = self
            .token_request("refresh_token", json!({ "refresh_token": token }))
            .await?;
        self.client.session_store.set(session.clone());
        Ok(session)
    }

    /// If the stored session expires within `threshold_secs`, refresh it.
    /// Returns the (possibly refreshed) session.
    pub async fn refresh_session_if_needed(&self, threshold_secs: i64) -> Result<Option<Session>> {
        let current = match self.client.session_store.get() {
            Some(s) => s,
            None => return Ok(None),
        };
        if current.expires_within(threshold_secs) {
            let refreshed = self.refresh_session(None).await?;
            Ok(Some(refreshed))
        } else {
            Ok(Some(current))
        }
    }

    /// Revoke the current session on the server and clear local state.
    pub async fn sign_out(&self, scope: SignOutScope) -> Result<()> {
        let session = self.client.session_store.get();
        if let Some(s) = &session {
            let opts = RequestOptions {
                bearer_override: Some(s.access_token.clone()),
                ..RequestOptions::auth()
            };
            let path = format!("/auth/v1/logout?scope={}", scope.as_str());
            // 204 No Content is normal here; the request layer treats it as Value::Null.
            let _ = self
                .client
                .request_with(&path, HttpMethod::Post, None, &opts)
                .await?;
        }
        self.client.session_store.clear();
        Ok(())
    }

    /// Internal: POST to `/auth/v1/token?grant_type=<grant>` and parse a Session.
    async fn token_request(&self, grant_type: &str, body: Value) -> Result<Session> {
        let path = format!("/auth/v1/token?grant_type={grant_type}");
        let value = self
            .client
            .request_with(&path, HttpMethod::Post, Some(body), &RequestOptions::auth())
            .await?;
        parse_session(value)
    }
}

/// Decode a GoTrue session payload, filling in `expires_at` if absent.
pub(crate) fn parse_session(value: Value) -> Result<Session> {
    let mut session: Session =
        serde_json::from_value(value.clone()).map_err(|e| SupabaseError::Decode {
            message: e.to_string(),
            body: value.to_string(),
        })?;
    session.fill_expires_at();
    Ok(session)
}

fn build_sign_up_body(email: &str, password: &str, opts: &SignUpOptions) -> Value {
    let mut body = json!({ "email": email, "password": password });
    if let Some(redirect) = &opts.email_redirect_to {
        body["email_redirect_to"] = json!(redirect);
    }
    if let Some(meta) = &opts.user_metadata {
        body["data"] = meta.clone();
    }
    if let Some(captcha) = &opts.captcha_token {
        body["gotrue_meta_security"] = json!({ "captcha_token": captcha });
    }
    if let Some(channel) = &opts.channel {
        body["channel"] = json!(channel);
    }
    body
}


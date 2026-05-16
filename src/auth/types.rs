//! Strongly-typed representations of GoTrue payloads.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A fully-decoded GoTrue user record. Matches the shape returned by `/auth/v1/user`
/// and `/auth/v1/admin/users` (with admin-only fields exposed via `Option`).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct User {
    pub id: String,
    #[serde(default)]
    pub aud: String,
    #[serde(default)]
    pub role: String,

    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub email_confirmed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub new_email: Option<String>,
    #[serde(default)]
    pub email_change_sent_at: Option<DateTime<Utc>>,

    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub phone_confirmed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub new_phone: Option<String>,
    #[serde(default)]
    pub phone_change_sent_at: Option<DateTime<Utc>>,

    #[serde(default)]
    pub confirmation_sent_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub confirmed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub recovery_sent_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub invited_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_sign_in_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub reauthentication_sent_at: Option<DateTime<Utc>>,

    #[serde(default)]
    pub app_metadata: Value,
    #[serde(default)]
    pub user_metadata: Value,
    /// GoTrue returns `"identities": null` for some users (e.g. anonymous);
    /// treat null as an empty list.
    #[serde(default, deserialize_with = "null_to_default")]
    pub identities: Vec<Identity>,
    #[serde(default)]
    pub factors: Option<Vec<Value>>,
    #[serde(default)]
    pub is_anonymous: bool,

    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

/// A login identity (one user may have many — e.g. email + Google).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Identity {
    pub id: String,
    pub user_id: String,
    #[serde(default)]
    pub identity_data: Value,
    #[serde(default)]
    pub identity_id: Option<String>,
    pub provider: String,
    #[serde(default)]
    pub last_sign_in_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

/// An authenticated session as returned by `/auth/v1/token`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Session {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    /// Unix timestamp in seconds. Computed locally if GoTrue omits it.
    #[serde(default)]
    pub expires_at: i64,
    pub refresh_token: String,
    pub user: User,
}

impl Session {
    /// Fill in `expires_at` from `expires_in` if the server didn't provide it.
    pub(crate) fn fill_expires_at(&mut self) {
        if self.expires_at == 0 {
            let now = Utc::now().timestamp();
            self.expires_at = now.saturating_add(self.expires_in);
        }
    }

    /// Has the access token expired according to its `expires_at`?
    pub fn is_expired(&self) -> bool {
        self.seconds_until_expiry() <= 0
    }

    /// Seconds remaining until the access token expires (negative if past).
    pub fn seconds_until_expiry(&self) -> i64 {
        self.expires_at - Utc::now().timestamp()
    }

    /// `true` when the access token has fewer than `threshold_secs` seconds of life left.
    pub fn expires_within(&self, threshold_secs: i64) -> bool {
        self.seconds_until_expiry() <= threshold_secs
    }
}

// --- request / option payloads --------------------------------------------

/// Options accepted by [`Auth::sign_up`](super::Auth::sign_up) and several other flows.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SignUpOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_redirect_to: Option<String>,
    /// Arbitrary user metadata stored on the resulting user.
    #[serde(rename = "data", skip_serializing_if = "Option::is_none")]
    pub user_metadata: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub captcha_token: Option<String>,
    /// Forwarded to GoTrue's optional channel switching (e.g. SMS).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
}

/// What to log in with for an OTP-based sign-in.
#[derive(Debug, Clone)]
pub enum OtpRecipient {
    Email(String),
    Phone(String),
}

/// Options for [`Auth::sign_in_with_otp`](super::Auth::sign_in_with_otp).
#[derive(Debug, Clone, Default)]
pub struct OtpOptions {
    pub email_redirect_to: Option<String>,
    pub should_create_user: Option<bool>,
    pub user_metadata: Option<Value>,
    pub captcha_token: Option<String>,
    pub channel: Option<String>,
}

/// Verification kinds for [`Auth::verify_otp`](super::Auth::verify_otp).
#[derive(Debug, Clone, Copy)]
pub enum OtpType {
    Signup,
    Invite,
    Magiclink,
    Recovery,
    EmailChange,
    Sms,
    PhoneChange,
    Email,
}

impl OtpType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Signup => "signup",
            Self::Invite => "invite",
            Self::Magiclink => "magiclink",
            Self::Recovery => "recovery",
            Self::EmailChange => "email_change",
            Self::Sms => "sms",
            Self::PhoneChange => "phone_change",
            Self::Email => "email",
        }
    }
}

/// Attributes accepted by [`Auth::update_user`](super::Auth::update_user).
#[derive(Debug, Clone, Default, Serialize)]
pub struct UpdateUserAttributes {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    #[serde(rename = "data", skip_serializing_if = "Option::is_none")]
    pub user_metadata: Option<Value>,
}

/// Attributes accepted by the admin user APIs.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AdminUserAttributes {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_confirm: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone_confirm: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ban_duration: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_metadata: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_metadata: Option<Value>,
}

/// Sign-out scope. `Global` revokes all sessions for the user, `Local` only the
/// current one, `Others` everything except the current session.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignOutScope {
    Global,
    Local,
    Others,
}

impl SignOutScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Local => "local",
            Self::Others => "others",
        }
    }
}

/// OAuth provider name. Strings are passed through to GoTrue so any provider
/// you've enabled in the dashboard is valid.
pub type OAuthProvider = String;

/// Options for [`Auth::sign_in_with_oauth`](super::Auth::sign_in_with_oauth).
#[derive(Debug, Clone, Default)]
pub struct OAuthOptions {
    pub redirect_to: Option<String>,
    /// Space-separated scopes (e.g. `"read:user user:email"`).
    pub scopes: Option<String>,
    pub query_params: HashMap<String, String>,
    pub skip_browser_redirect: bool,
}

/// Result of an OAuth sign-in: the URL the user must visit to complete
/// authentication.
#[derive(Debug, Clone)]
pub struct OAuthFlow {
    pub provider: OAuthProvider,
    pub url: String,
}

/// Options for [`Auth::reset_password_for_email`](super::Auth::reset_password_for_email).
#[derive(Debug, Clone, Default)]
pub struct ResetPasswordOptions {
    pub redirect_to: Option<String>,
    pub captcha_token: Option<String>,
}

/// Parameters for [`Auth::verify_otp`](super::Auth::verify_otp).
#[derive(Debug, Clone)]
pub enum VerifyOtpParams {
    Email { email: String, token: String, otp_type: OtpType },
    Phone { phone: String, token: String, otp_type: OtpType },
    TokenHash { token_hash: String, otp_type: OtpType },
}

/// Treat a `null` JSON value as `T::default()` during deserialization.
///
/// Useful when an API spec says a field is non-null but the server sometimes
/// emits `null` (e.g. GoTrue's `identities` field for anonymous users).
fn null_to_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    T: Default + serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    let opt = Option::<T>::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    fn make_session(seconds_from_now: i64) -> Session {
        Session {
            access_token: "at".into(),
            token_type: "bearer".into(),
            expires_in: seconds_from_now,
            expires_at: Utc::now().timestamp() + seconds_from_now,
            refresh_token: "rt".into(),
            user: serde_json::from_value(json!({
                "id": "u1",
                "aud": "authenticated",
                "role": "authenticated",
                "created_at": "2024-01-01T00:00:00Z"
            }))
            .unwrap(),
        }
    }

    #[test]
    fn session_expiry_math() {
        let s = make_session(120);
        assert!(!s.is_expired());
        assert!(s.expires_within(180));
        assert!(!s.expires_within(60));
        let stale = make_session(-10);
        assert!(stale.is_expired());
    }

    #[test]
    fn fill_expires_at_when_missing() {
        let mut s = Session {
            access_token: "at".into(),
            token_type: "bearer".into(),
            expires_in: 3600,
            expires_at: 0,
            refresh_token: "rt".into(),
            user: serde_json::from_value(json!({
                "id": "u1",
                "aud": "authenticated",
                "role": "authenticated",
                "created_at": "2024-01-01T00:00:00Z"
            }))
            .unwrap(),
        };
        s.fill_expires_at();
        let now = Utc::now().timestamp();
        assert!(s.expires_at >= now + 3590 && s.expires_at <= now + 3610);
    }

    #[test]
    fn user_deserializes_with_optional_fields_missing() {
        let v = json!({
            "id": "abc",
            "aud": "authenticated",
            "role": "authenticated",
            "created_at": "2024-01-01T00:00:00Z"
        });
        let user: User = serde_json::from_value(v).unwrap();
        assert_eq!(user.id, "abc");
        assert!(user.email.is_none());
        assert!(user.identities.is_empty());
        assert!(!user.is_anonymous);
    }

    #[test]
    fn user_deserializes_full_payload() {
        let v = json!({
            "id": "u1",
            "aud": "authenticated",
            "role": "authenticated",
            "email": "x@y.com",
            "email_confirmed_at": "2024-03-04T10:00:00Z",
            "phone": "",
            "last_sign_in_at": "2024-03-05T12:00:00Z",
            "app_metadata": { "provider": "email", "providers": ["email"] },
            "user_metadata": { "name": "Alice" },
            "identities": [
                {
                    "id": "i1",
                    "user_id": "u1",
                    "identity_data": { "email": "x@y.com" },
                    "provider": "email",
                    "created_at": "2024-01-01T00:00:00Z"
                }
            ],
            "is_anonymous": false,
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-03-05T12:00:00Z"
        });
        let user: User = serde_json::from_value(v).unwrap();
        assert_eq!(user.email.as_deref(), Some("x@y.com"));
        assert_eq!(user.identities.len(), 1);
        assert_eq!(user.identities[0].provider, "email");
        assert_eq!(user.user_metadata["name"], "Alice");
    }

    // --- additional Session tests ---

    #[test]
    fn session_is_expired_when_past() {
        let s = make_session(-1);
        assert!(s.is_expired(), "session with -1s remaining should be expired");
    }

    #[test]
    fn session_not_expired_when_future() {
        let s = make_session(3600);
        assert!(!s.is_expired());
    }

    #[test]
    fn session_expires_within_boundary() {
        let s = make_session(100);
        assert!(s.expires_within(100), "expires_within(100) when 100s left should be true");
        assert!(s.expires_within(200), "expires_within(200) when 100s left should be true");
        assert!(!s.expires_within(99), "expires_within(99) when 100s left should be false");
    }

    #[test]
    fn session_seconds_until_expiry_positive() {
        let s = make_session(500);
        let rem = s.seconds_until_expiry();
        assert!(rem > 0 && rem <= 500, "seconds_until_expiry={rem}");
    }

    #[test]
    fn session_seconds_until_expiry_negative() {
        let s = make_session(-60);
        let rem = s.seconds_until_expiry();
        assert!(rem < 0, "should be negative, got {rem}");
    }

    #[test]
    fn session_fill_expires_at_idempotent_when_already_set() {
        let ts = Utc::now().timestamp() + 3600;
        let mut s = Session {
            access_token: "t".into(),
            token_type: "bearer".into(),
            expires_in: 7200,
            expires_at: ts,
            refresh_token: "r".into(),
            user: serde_json::from_value(json!({
                "id": "u", "aud": "a", "role": "r", "created_at": "2024-01-01T00:00:00Z"
            })).unwrap(),
        };
        s.fill_expires_at();
        // Should stay at ts (3600 from now), NOT be reset to 7200 from now.
        assert!((s.expires_at - ts).abs() <= 1, "expires_at should not be overwritten");
    }

    // --- OAuthProvider ---
    // OAuthProvider is a String type alias — any provider name the GoTrue
    // server knows is valid (github, google, apple, slack, …).

    #[test]
    fn oauth_provider_is_a_string() {
        let p: OAuthProvider = "github".to_string();
        assert_eq!(p, "github");
    }

    #[test]
    fn oauth_provider_round_trips_via_json() {
        let p: OAuthProvider = "google".to_string();
        let serialized = serde_json::to_string(&p).unwrap();
        assert_eq!(serialized, r#""google""#);
        let back: OAuthProvider = serde_json::from_str(&serialized).unwrap();
        assert_eq!(back, "google");
    }

    #[test]
    fn oauth_options_accepts_provider_string() {
        let opts = OAuthOptions {
            redirect_to: Some("https://example.com/callback".into()),
            scopes: Some("read:user".into()),
            ..Default::default()
        };
        // OAuthFlow carries the provider string through.
        let flow = OAuthFlow {
            provider: "github".to_string(),
            url: "https://accounts.github.com/oauth/authorize?...".to_string(),
        };
        assert_eq!(flow.provider, "github");
        assert_eq!(opts.redirect_to.as_deref(), Some("https://example.com/callback"));
    }

    // --- SignOutScope ---

    #[test]
    fn sign_out_scope_as_str() {
        assert_eq!(SignOutScope::Global.as_str(), "global");
        assert_eq!(SignOutScope::Local.as_str(), "local");
        assert_eq!(SignOutScope::Others.as_str(), "others");
    }

    #[test]
    fn sign_out_scope_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&SignOutScope::Global).unwrap(),
            r#""global""#
        );
        assert_eq!(
            serde_json::to_string(&SignOutScope::Local).unwrap(),
            r#""local""#
        );
        assert_eq!(
            serde_json::to_string(&SignOutScope::Others).unwrap(),
            r#""others""#
        );
    }

    #[test]
    fn sign_out_scope_deserializes() {
        let s: SignOutScope = serde_json::from_str(r#""global""#).unwrap();
        assert!(matches!(s, SignOutScope::Global));
        let s: SignOutScope = serde_json::from_str(r#""others""#).unwrap();
        assert!(matches!(s, SignOutScope::Others));
    }

    // --- User anonymous flag ---

    #[test]
    fn user_is_anonymous_defaults_to_false() {
        let v = json!({"id": "u", "aud": "a", "role": "r", "created_at": "2024-01-01T00:00:00Z"});
        let u: User = serde_json::from_value(v).unwrap();
        assert!(!u.is_anonymous);
    }

    #[test]
    fn user_is_anonymous_true_when_set() {
        let v = json!({
            "id": "u", "aud": "a", "role": "r",
            "created_at": "2024-01-01T00:00:00Z",
            "is_anonymous": true
        });
        let u: User = serde_json::from_value(v).unwrap();
        assert!(u.is_anonymous);
    }

    // --- Identity deserialization ---

    #[test]
    fn identity_deserializes() {
        let v = json!({
            "id": "id1",
            "user_id": "u1",
            "identity_data": { "sub": "abc" },
            "provider": "google",
            "created_at": "2024-01-01T00:00:00Z"
        });
        let i: Identity = serde_json::from_value(v).unwrap();
        assert_eq!(i.provider, "google");
        assert_eq!(i.user_id, "u1");
        assert_eq!(i.identity_data["sub"], "abc");
    }

    // --- OtpType::as_str ---

    #[test]
    fn otp_type_as_str_all_variants() {
        assert_eq!(OtpType::Signup.as_str(), "signup");
        assert_eq!(OtpType::Invite.as_str(), "invite");
        assert_eq!(OtpType::Magiclink.as_str(), "magiclink");
        assert_eq!(OtpType::Recovery.as_str(), "recovery");
        assert_eq!(OtpType::EmailChange.as_str(), "email_change");
        assert_eq!(OtpType::Sms.as_str(), "sms");
        assert_eq!(OtpType::PhoneChange.as_str(), "phone_change");
        assert_eq!(OtpType::Email.as_str(), "email");
    }

    // --- SignUpOptions serialization ---

    #[test]
    fn sign_up_options_skips_none_fields() {
        let opts = SignUpOptions::default();
        let v = serde_json::to_value(&opts).unwrap();
        assert!(v.as_object().unwrap().is_empty());
    }

    #[test]
    fn sign_up_options_serializes_set_fields() {
        let opts = SignUpOptions {
            email_redirect_to: Some("https://example.com/callback".into()),
            user_metadata: Some(json!({"name": "Alice"})),
            captcha_token: Some("captcha-tok".into()),
            channel: Some("sms".into()),
        };
        let v = serde_json::to_value(&opts).unwrap();
        assert_eq!(v["email_redirect_to"], "https://example.com/callback");
        assert_eq!(v["data"]["name"], "Alice");   // renamed to "data"
        assert_eq!(v["captcha_token"], "captcha-tok");
        assert_eq!(v["channel"], "sms");
    }

    // --- UpdateUserAttributes serialization ---

    #[test]
    fn update_user_attrs_skips_none() {
        let v = serde_json::to_value(UpdateUserAttributes::default()).unwrap();
        assert!(v.as_object().unwrap().is_empty());
    }

    #[test]
    fn update_user_attrs_serializes_password() {
        let attrs = UpdateUserAttributes {
            password: Some("s3cr3t".into()),
            ..Default::default()
        };
        let v = serde_json::to_value(&attrs).unwrap();
        assert_eq!(v["password"], "s3cr3t");
        assert!(v.get("email").is_none());
    }

    #[test]
    fn update_user_attrs_metadata_renamed_to_data() {
        let attrs = UpdateUserAttributes {
            user_metadata: Some(json!({"theme": "dark"})),
            ..Default::default()
        };
        let v = serde_json::to_value(&attrs).unwrap();
        assert_eq!(v["data"]["theme"], "dark");
    }

    // --- VerifyOtpParams variants ---

    #[test]
    fn verify_otp_params_email_variant() {
        let p = VerifyOtpParams::Email {
            email: "x@y.com".into(),
            token: "123456".into(),
            otp_type: OtpType::Signup,
        };
        match p {
            VerifyOtpParams::Email { email, token, otp_type } => {
                assert_eq!(email, "x@y.com");
                assert_eq!(token, "123456");
                assert_eq!(otp_type.as_str(), "signup");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn verify_otp_params_phone_variant() {
        let p = VerifyOtpParams::Phone {
            phone: "+1555000".into(),
            token: "654321".into(),
            otp_type: OtpType::Sms,
        };
        match p {
            VerifyOtpParams::Phone { phone, token, .. } => {
                assert_eq!(phone, "+1555000");
                assert_eq!(token, "654321");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn verify_otp_params_token_hash_variant() {
        let p = VerifyOtpParams::TokenHash {
            token_hash: "abc123".into(),
            otp_type: OtpType::Recovery,
        };
        match p {
            VerifyOtpParams::TokenHash { token_hash, otp_type } => {
                assert_eq!(token_hash, "abc123");
                assert_eq!(otp_type.as_str(), "recovery");
            }
            _ => panic!("wrong variant"),
        }
    }
}

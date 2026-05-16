//! OAuth helpers — constructs the authorization URL for the `authorize` flow.

use super::types::{OAuthFlow, OAuthOptions, OAuthProvider};

/// Build the URL the user should be redirected to in order to start an OAuth flow.
///
/// This is the non-PKCE flow. After the provider redirects back to your app,
/// pass the resulting `code` to [`Auth::exchange_code_for_session`](super::Auth::exchange_code_for_session).
pub(crate) fn build_authorize_url(
    supabase_url: &str,
    provider: OAuthProvider,
    options: OAuthOptions,
) -> OAuthFlow {
    let mut params: Vec<(String, String)> = vec![("provider".to_string(), provider.clone())];

    if let Some(redirect) = options.redirect_to {
        params.push(("redirect_to".to_string(), redirect));
    }
    if let Some(scopes) = options.scopes {
        params.push(("scopes".to_string(), scopes));
    }
    for (k, v) in options.query_params {
        params.push((k, v));
    }

    let query: Vec<String> = params
        .into_iter()
        .map(|(k, v)| format!("{}={}", urlencoding::encode(&k), urlencoding::encode(&v)))
        .collect();

    let url = format!("{}/auth/v1/authorize?{}", supabase_url, query.join("&"));
    OAuthFlow { provider, url }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn basic_provider_only() {
        let flow = build_authorize_url(
            "https://example.supabase.co",
            "github".into(),
            OAuthOptions::default(),
        );
        assert_eq!(flow.provider, "github");
        assert_eq!(
            flow.url,
            "https://example.supabase.co/auth/v1/authorize?provider=github"
        );
    }

    #[test]
    fn redirect_and_scopes() {
        let flow = build_authorize_url(
            "https://example.supabase.co",
            "google".into(),
            OAuthOptions {
                redirect_to: Some("https://app.example.com/callback".into()),
                scopes: Some("openid email profile".into()),
                ..Default::default()
            },
        );
        assert!(flow.url.starts_with("https://example.supabase.co/auth/v1/authorize?"));
        assert!(flow.url.contains("provider=google"));
        assert!(flow
            .url
            .contains("redirect_to=https%3A%2F%2Fapp.example.com%2Fcallback"));
        assert!(flow.url.contains("scopes=openid%20email%20profile"));
    }

    #[test]
    fn custom_query_params() {
        let mut params = HashMap::new();
        params.insert("access_type".to_string(), "offline".to_string());
        params.insert("prompt".to_string(), "consent".to_string());
        let flow = build_authorize_url(
            "https://example.supabase.co",
            "google".into(),
            OAuthOptions {
                query_params: params,
                ..Default::default()
            },
        );
        assert!(flow.url.contains("access_type=offline"));
        assert!(flow.url.contains("prompt=consent"));
    }
}

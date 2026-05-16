//! `cargo run --example auth_email`
//!
//! Sign in with email + password, fetch the user, then sign out.

use rust_supabase_sdk::auth::SignOutScope;
use rust_supabase_sdk::SupabaseClient;

#[tokio::main]
async fn main() -> rust_supabase_sdk::Result<()> {
    let url = std::env::var("SUPABASE_URL").map_err(|_| {
        rust_supabase_sdk::SupabaseError::Unexpected("SUPABASE_URL not set".into())
    })?;
    let key = std::env::var("SUPABASE_API_KEY").map_err(|_| {
        rust_supabase_sdk::SupabaseError::Unexpected("SUPABASE_API_KEY not set".into())
    })?;
    let email = std::env::var("DEMO_EMAIL").map_err(|_| {
        rust_supabase_sdk::SupabaseError::Unexpected("DEMO_EMAIL not set".into())
    })?;
    let password = std::env::var("DEMO_PASSWORD").map_err(|_| {
        rust_supabase_sdk::SupabaseError::Unexpected("DEMO_PASSWORD not set".into())
    })?;

    let client = SupabaseClient::new(url, key, None);

    let session = client.auth().sign_in_with_password(&email, &password).await?;
    println!("signed in as {} (expires in {}s)", session.user.id, session.expires_in);

    let user = client.auth().get_user().await?;
    println!("user email: {:?}", user.email);

    client.auth().sign_out(SignOutScope::Global).await?;
    println!("signed out");
    Ok(())
}

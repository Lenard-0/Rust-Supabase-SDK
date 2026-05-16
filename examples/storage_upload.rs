//! `cargo run --example storage_upload`
//!
//! Upload a small payload to a Storage bucket, then print its public URL.

use rust_supabase_sdk::storage::UploadOptions;
use rust_supabase_sdk::SupabaseClient;

#[tokio::main]
async fn main() -> rust_supabase_sdk::Result<()> {
    let url = std::env::var("SUPABASE_URL").map_err(|_| {
        rust_supabase_sdk::SupabaseError::Unexpected("SUPABASE_URL not set".into())
    })?;
    let key = std::env::var("SUPABASE_API_KEY").map_err(|_| {
        rust_supabase_sdk::SupabaseError::Unexpected("SUPABASE_API_KEY not set".into())
    })?;
    let bucket = std::env::var("DEMO_BUCKET").unwrap_or_else(|_| "public".into());

    let client = SupabaseClient::new(url, key, None);
    let api = client.storage().from(&bucket);

    let path = format!("demo/{}.txt", uuid_simple());
    api.upload(
        &path,
        b"hello from rust-supabase-sdk\n".to_vec(),
        UploadOptions { content_type: Some("text/plain".into()), upsert: true, ..Default::default() },
    )
    .await?;

    let public = api.get_public_url(&path, Default::default());
    println!("uploaded -> {public}");
    Ok(())
}

fn uuid_simple() -> String {
    rust_supabase_sdk::generate_id()
}

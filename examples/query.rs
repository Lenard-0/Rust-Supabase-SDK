//! `cargo run --example query`
//!
//! Reads SUPABASE_URL and SUPABASE_API_KEY from the environment and runs a
//! filtered SELECT against the `countries` table.

use rust_supabase_sdk::SupabaseClient;
use serde_json::Value;

#[tokio::main]
async fn main() -> rust_supabase_sdk::Result<()> {
    let url = std::env::var("SUPABASE_URL").map_err(|_| {
        rust_supabase_sdk::SupabaseError::Unexpected("SUPABASE_URL not set".into())
    })?;
    let key = std::env::var("SUPABASE_API_KEY").map_err(|_| {
        rust_supabase_sdk::SupabaseError::Unexpected("SUPABASE_API_KEY not set".into())
    })?;

    let client = SupabaseClient::new(url, key, None);

    let rows: Vec<Value> = client
        .from("countries")
        .select("id,name")
        .order("name", true)
        .limit(10)
        .await?;

    for row in rows {
        println!("{}", row);
    }
    Ok(())
}

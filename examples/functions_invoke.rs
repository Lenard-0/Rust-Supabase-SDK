//! `cargo run --example functions_invoke`
//!
//! Invoke a deployed Edge Function. Demonstrates the JSON fast path
//! (`Functions::invoke`) and the full-options path
//! (`Functions::invoke_with`), plus the streaming variant.
//!
//! Set `SUPABASE_URL`, `SUPABASE_API_KEY`, and (optionally) `FUNCTION_NAME`
//! (defaults to `hello`). The function should accept JSON `{ "name": "<str>" }`
//! and return JSON `{ "message": "<str>" }`.

use rust_supabase_sdk::functions::{FunctionRegion, InvokeMethod, InvokeOptions};
use rust_supabase_sdk::SupabaseClient;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct HelloRequest<'a> {
    name: &'a str,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)] // fields are surfaced via Debug print only.
struct HelloResponse {
    #[serde(default)]
    message: String,
}

#[tokio::main]
async fn main() -> rust_supabase_sdk::Result<()> {
    let url = std::env::var("SUPABASE_URL").map_err(|_| {
        rust_supabase_sdk::SupabaseError::Unexpected("SUPABASE_URL not set".into())
    })?;
    let key = std::env::var("SUPABASE_API_KEY").map_err(|_| {
        rust_supabase_sdk::SupabaseError::Unexpected("SUPABASE_API_KEY not set".into())
    })?;
    let name = std::env::var("FUNCTION_NAME").unwrap_or_else(|_| "hello".to_string());

    let client = SupabaseClient::new(url, key, None);

    // 1. JSON fast path.
    let res: HelloResponse = client
        .functions()
        .invoke(&name, &HelloRequest { name: "world" })
        .await?;
    println!("fast-path → {res:?}");

    // 2. Full options: custom header + region + method.
    let opts = InvokeOptions::new()
        .body_json(&HelloRequest { name: "rust" })?
        .header("X-Demo", "1")
        .region(FunctionRegion::UsEast1)
        .method(InvokeMethod::Post);
    let res: HelloResponse = client.functions().invoke_with(&name, opts).await?;
    println!("with-options → {res:?}");

    // 3. Streaming: read the body as raw bytes (e.g. for SSE or large payloads).
    let stream_opts = InvokeOptions::new().body_json(&HelloRequest { name: "stream" })?;
    let resp = client.functions().invoke_stream(&name, stream_opts).await?;
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("?")
        .to_string();
    let bytes = resp.bytes().await.map_err(rust_supabase_sdk::SupabaseError::from)?;
    println!("stream ({content_type}) → {} bytes", bytes.len());

    Ok(())
}

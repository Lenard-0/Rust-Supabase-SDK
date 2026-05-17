# rust_supabase_sdk

[![crates.io](https://img.shields.io/crates/v/rust_supabase_sdk.svg)](https://crates.io/crates/rust_supabase_sdk)
[![docs.rs](https://img.shields.io/docsrs/rust_supabase_sdk)](https://docs.rs/rust_supabase_sdk)
[![license](https://img.shields.io/crates/l/rust_supabase_sdk.svg)](LICENSE)

An ergonomic, async Rust client for [Supabase](https://supabase.com).

Mirrors the `supabase-js` surface area where it makes sense and pushes
Rust-native ergonomics elsewhere:

- **PostgREST** — chainable query builder + typed row queries via `from_row::<T>()`
- **Auth** — email / phone / OTP / OAuth / anonymous sign-in, account recovery, admin user management, pluggable session stores
- **Storage** — buckets, object CRUD, signed URLs, image transforms
- **RPC** — call Postgres functions with `rpc_call(...)`
- **Edge Functions** — invoke deployed functions, streaming responses supported
- **Realtime** — websocket subscriptions to `postgres_changes`, broadcast, and presence (opt-in feature)
- **Retry** — automatic exponential backoff on 429 / 5xx

## Installation

```toml
[dependencies]
rust_supabase_sdk = "0.4"
```

**MSRV:** Rust 1.75.

## Quickstart

```rust
use rust_supabase_sdk::SupabaseClient;

#[tokio::main]
async fn main() -> rust_supabase_sdk::Result<()> {
    let client = SupabaseClient::new(
        std::env::var("SUPABASE_URL").unwrap_or_default(),
        std::env::var("SUPABASE_API_KEY").unwrap_or_default(),
        None,
    );

    let rows: Vec<serde_json::Value> = client
        .from("countries")
        .select("id,name")
        .eq("region", "Europe")
        .order("name", true)
        .limit(10)
        .await?;

    for row in rows {
        println!("{row}");
    }
    Ok(())
}
```

## Feature flags

| Feature      | Default | Notes                                       |
|--------------|:-------:|---------------------------------------------|
| `postgrest`  | ✅      | Chainable query builder.                    |
| `auth`       | ✅      | Sign-in flows, OAuth, admin user management. |
| `storage`    | ✅      | Buckets + objects + signed URLs.            |
| `functions`  | ✅      | Edge Functions invocation.                  |
| `realtime`   | —       | Websocket subscriptions (opt-in).           |
| `rustls`     | ✅      | TLS via rustls (default).                   |
| `native-tls` | —       | Use OS TLS instead of rustls.               |

Enable `realtime` explicitly:

```toml
rust_supabase_sdk = { version = "0.4", features = ["realtime"] }
```

## Customizing the client

```rust
use std::time::Duration;
use rust_supabase_sdk::{SupabaseClient, RetryConfig};

let client = SupabaseClient::builder(url, key)
    .timeout(Duration::from_secs(30))
    .retry(RetryConfig::new(3, Duration::from_millis(100)))
    .user_agent("my-app/1.0")
    .schema("public")
    .build();
```

`SupabaseClient` is cheap to `clone` — internal state is `Arc`-shared, so a single
configured client can be passed across tasks and modules.

## Typed queries

Implement `Row` for your row type (or generate it — see below) and use
`from_row::<T>()` for end-to-end type safety:

```rust,ignore
use rust_supabase_sdk::Row;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Country { id: i64, name: String }

impl Row for Country {
    const TABLE: &'static str = "countries";
    const SCHEMA: Option<&'static str> = Some("public");
    const COLUMNS: &'static [&'static str] = &["id", "name"];
}

let rows: Vec<Country> = client.from_row::<Country>()
    .eq("region", "Europe")
    .await?;
```

## Code generation

The companion `cargo-supabase` binary introspects a Supabase project's PostgREST
schema and emits Rust row structs (with `Row` impls) ready for use with
`from_row::<T>()`:

```sh
cargo install --path cargo-supabase   # one-time

cargo supabase gen types \
    --url    "$SUPABASE_URL" \
    --apikey "$SUPABASE_API_KEY" \
    --output src/db.rs
```

Re-run whenever the DB schema changes — drift becomes a compile error rather than a runtime failure.

See [docs/codegen.md](docs/codegen.md) for the full flag reference, type mapping table, and worked examples.

## Testing

Run the test suite:

```sh
cargo test
```

### Code coverage

Install [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov) once:

```sh
cargo install cargo-llvm-cov
```

| Command | Output |
|---------|--------|
| `cargo llvm-cov` | Summary in terminal |
| `cargo llvm-cov --html` | HTML report in `target/llvm-cov/html/` |
| `cargo llvm-cov --open` | HTML report, opened in browser |
| `cargo llvm-cov --lcov --output-path lcov.info` | LCOV file for CI / coverage services |

## Examples

Worked examples for every major surface area:

```
cargo run --example query
cargo run --example postgrest_typed
cargo run --example auth_email
cargo run --example storage_upload
cargo run --example functions_invoke
cargo run --example realtime_changes --features realtime
```

All examples read `SUPABASE_URL` and `SUPABASE_API_KEY` from the environment.

## Documentation

Full API documentation lives on [docs.rs](https://docs.rs/rust_supabase_sdk).

## Contributing

Bug reports, feature requests, and PRs welcome at
[github.com/Lenard-0/Rust-Supabase-SDK](https://github.com/Lenard-0/Rust-Supabase-SDK).

## License

MIT — see [LICENSE](LICENSE).

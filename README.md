# rust_supabase_sdk

[![crates.io](https://img.shields.io/crates/v/rust_supabase_sdk.svg)](https://crates.io/crates/rust_supabase_sdk)
[![docs.rs](https://img.shields.io/docsrs/rust_supabase_sdk)](https://docs.rs/rust_supabase_sdk)
[![license](https://img.shields.io/crates/l/rust_supabase_sdk.svg)](LICENSE)

An ergonomic, async Rust client for [Supabase](https://supabase.com).

Mirrors the `supabase-js` surface area where it makes sense and pushes
Rust-native ergonomics elsewhere:

- **PostgREST** — chainable query builder (string-typed) **and** compile-time-checked typed queries via `from_row::<T>()` + codegen-emitted `Column<R, V>` constants
- **Auth** — email / phone / OTP / OAuth / anonymous sign-in, account recovery, admin user management, pluggable session stores
- **Storage** — buckets, object CRUD, signed URLs, image transforms
- **RPC** — call Postgres functions with `rpc_call(...)`
- **Edge Functions** — invoke deployed functions, streaming responses supported
- **Realtime** — websocket subscriptions to `postgres_changes`, broadcast, and presence (opt-in feature)
- **Retry** — automatic exponential backoff on 429 / 5xx

## Type safety that catches schema drift before you ship

Two query paths, both first-class. Start with the string-typed builder (zero
setup, supabase-js parity). Opt into typed columns whenever you want the
**compiler** to reject wrong column names and wrong value types.

```rust,ignore
// String path — supabase-js parity, no codegen required
let rows: Vec<Value> = client
    .from("posts")
    .select("*")
    .eq("status", "published")
    .gt("view_count", 100)
    .await?;

// Typed path — same query, every column + value checked at compile time
let rows: Vec<Posts> = client
    .from_row::<Posts>()
    .eq(Posts::status, "published".to_string())
    .gt(Posts::view_count, 100i32)
    .is_null(Posts::archived)
    .execute()
    .await?;
```

`Posts` and its column constants (`Posts::status`, `Posts::view_count`, …) are
emitted by `cargo supabase gen types` — re-run after a migration and any drift
becomes a compile error. None of the following code will build:

```text
.eq(Users::id, "x")                  // ✗ wrong row type
.eq(Posts::view_count, "abc")        // ✗ view_count is i32, not &str
.is_null(Posts::status)              // ✗ status is NOT NULL — is_null requires Option<_>
.like(Posts::view_count, "10%")      // ✗ like requires a string-typed column
```

Runtime cost is zero — `Column<R, V>` is a `&'static str` plus a phantom type.
[Full design and method list →](#typed-queries)

## Installation

```toml
[dependencies]
rust_supabase_sdk = "0.4.2"
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
rust_supabase_sdk = { version = "0.4.2", features = ["realtime"] }
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

The string and typed paths share the same client and the same wire protocol.
Pick per query:

| Entry point | Use when | Setup |
|---|---|---|
| `client.from("posts")` | Ad-hoc queries, views, computed columns, JSON paths, anything codegen can't see | None |
| `client.from_row::<Posts>()` | You want the compiler to verify column names and value types | `cargo supabase gen types` (or hand-rolled `Row` + column constants) |

### Codegen output (you don't write this)

```rust,ignore
use rust_supabase_sdk::{postgrest::Column, Row};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Posts {
    pub id: String,
    pub status: String,
    pub view_count: i32,
    pub archived: Option<bool>,
}
impl Row for Posts {
    const TABLE: &'static str = "posts";
}
#[allow(non_upper_case_globals)]
impl Posts {
    pub const id:         Column<Posts, String>       = Column::new("id");
    pub const status:     Column<Posts, String>       = Column::new("status");
    pub const view_count: Column<Posts, i32>          = Column::new("view_count");
    pub const archived:   Column<Posts, Option<bool>> = Column::new("archived");
}
```

### Available filters on the typed builder

`eq`, `neq`, `not_eq`, `gt`, `gte`, `lt`, `lte`, `like`, `ilike`, `not_like`,
`not_ilike`, `is_null`, `is_not_null`, `is_bool`, `in_`, `is_in`, `not_in_`,
`is_not_in`, `contains`, `contained_by`, `overlaps`, `order`, `order_with`,
`limit`, `offset`, `range`, `count`, `text_search`. Execution: `execute`,
`execute_with_count`, `single`, `maybe_single`. Escape hatch: `.into_untyped()`
drops to the string-typed `PostgrestBuilder` if you need an operation the
typed surface doesn't cover.

`is_in` is the recommended set-membership filter — it compiles to
`column=in.(v1,v2,v3)` and short-circuits empty input to `Ok(vec![])` without
hitting the wire (matches SQL semantics; avoids PostgREST's 400 on
`column=in.()`). Use it for batched primary-key fetches:

```rust,ignore
let chunks: Vec<DocumentChunks> = client
    .from_row::<DocumentChunks>()
    .is_in(DocumentChunks::id, chunk_ids)
    .execute()
    .await?;
```

### When the compiler rejects a query

| Misuse | Why |
|---|---|
| `eq(Users::id, "x")` inside `from_row::<Posts>()` | Column carries its row type — `Users::id` is `Column<Users, _>` |
| `eq(Posts::view_count, "abc")` | `view_count` is `Column<Posts, i32>`, value must be `i32` |
| `is_null(Posts::status)` | `status` is `String` (NOT NULL); `is_null` requires `Column<R, Option<V>>` |
| `like(Posts::view_count, "10%")` | `like` only takes `Column<R, String>` |
| `gt(Posts::status, 1i32)` | Value type must match the column's declared type |

Each check is codified as a compile-fail fixture under `tests/trybuild/typed-columns/`.

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

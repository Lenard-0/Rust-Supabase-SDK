# DB Codegen

`cargo-supabase` introspects a live Supabase project and emits Rust row structs with `Row` impls, ready for use with `from_row::<T>()`.

## Installation

```sh
cargo install --path cargo-supabase   # one-time
```

## Usage

```sh
cargo supabase gen types \
    --url    "$SUPABASE_URL" \
    --apikey "$SUPABASE_API_KEY" \
    --output src/db.rs
```

`--url` and `--apikey` are optional when `SUPABASE_URL` / `SUPABASE_API_KEY` are already exported in the environment.

### Flags

| Flag | Description |
|------|-------------|
| `--schema <NAME>` | Target schema (default: `public`) |
| `--only <TABLE>` | Include only these tables (repeatable) |
| `--exclude <TABLE>` | Exclude these tables (repeatable) |
| `--no-chrono` | Emit `String` for timestamps instead of `chrono::DateTime<Utc>` |
| `--uuid` | Emit `uuid::Uuid` for UUID columns instead of `String` |

**Examples:**

```sh
# private schema only
cargo supabase gen types --schema private --output src/db.rs

# specific tables
cargo supabase gen types --only users --only posts --output src/db.rs

# exclude audit tables, no chrono dependency
cargo supabase gen types --exclude audit_log --no-chrono --output src/db.rs
```

## How it works

1. Hits `<url>/rest/v1/` with your API key — PostgREST returns an OpenAPI document describing every table, column, and type in the schema.
2. Walks the definitions and maps PG types to Rust types:

   | Postgres | Rust |
   |----------|------|
   | `int4` | `i32` |
   | `text` | `String` |
   | `timestamptz` | `chrono::DateTime<Utc>` |
   | `_text` (array) | `Vec<String>` |
   | unknown | `serde_json::Value` |

3. Emits a single Rust module: one `struct` per table (`Serialize` + `Deserialize` derived) plus an `impl Row for Foo { const TABLE = "foo"; const COLUMNS = &[…]; }`.

## What you get

```rust
use crate::db::Posts;

let rows: Vec<Posts> = client
    .from_row::<Posts>()
    .select("*")
    .eq("status", "published")
    .execute()
    .await?;
```

- `from_row::<T>()` reads `T::TABLE` so the table name is type-checked rather than stringly-typed.
- Optional columns become `Option<T>`, required ones don't.
- Rust keywords (`type`, `match`, …) get `r#` prefixes automatically.

## Keeping in sync

Re-run codegen any time the DB schema changes and recompile. Schema drift becomes a compile error rather than a runtime decode failure.

```sh
cargo supabase gen types --output src/db.rs && cargo build
```

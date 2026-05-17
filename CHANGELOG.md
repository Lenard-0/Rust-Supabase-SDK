# Changelog

All notable changes follow [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.4.0] - 2026-05-17

### Type-safe column references — `Column<R, V>` + `TypedBuilder<R>`

`from_row::<R>()` now returns a fully typed query builder. Columns emitted
by the codegen carry both their row type and their value type at the type
level, so wrong-table, wrong-value-type, nullable-only, and string-only
filters are caught by the compiler instead of by PostgREST at runtime.

The untyped, string-keyed builder (`client.from(table).eq("col", val)`)
is **unchanged and remains first-class** — typed columns are an opt-in
upgrade, not a replacement.

#### Added

- **`postgrest::Column<R, V>`** — zero-sized, `Copy` column reference
  carrying a row type `R` and a value type `V`. Internally a
  `&'static str` plus a phantom marker — runtime cost is zero.
- **`postgrest::IntoColumnName<R>`** — trait that abstracts over typed
  columns and `&str` for the cases where value-type checking isn't
  valuable (currently used for `order` / `order_with`, where the
  argument is a sort key rather than a filter value).
- **`postgrest::TypedBuilder<R>`** — type-safe query builder with these
  methods: `select`, `eq`, `neq`, `not_eq`, `gt`, `gte`, `lt`, `lte`,
  `like`, `ilike`, `not_like`, `not_ilike`, `is_null`, `is_not_null`,
  `is_bool`, `in_`, `not_in_`, `contains`, `contained_by`, `overlaps`,
  `order`, `order_with`, `limit`, `offset`, `range`, `count`,
  `text_search`, `execute`, `execute_with_count`, `single`,
  `maybe_single`, `returns`, `into_untyped`, `build_path`.
- **Codegen** now emits a column-constants block alongside every row
  struct:
  ```rust
  #[allow(non_upper_case_globals)]
  impl Posts {
      pub const id: Column<Posts, i64> = Column::new("id");
      pub const status: Column<Posts, Option<String>> = Column::new("status");
      // ...
  }
  ```

### Breaking

- **`SupabaseClient::from_row::<R>()` now returns `TypedBuilder<R>`**
  (previously returned `TableBuilder`). Callers that want the untyped
  chain should either use `client.from(R::TABLE)` directly, or call
  `.into_untyped()` on the typed builder to drop back to the previous
  shape.

#### Migration

```text
old                                                                  →  new
─────────────────────────────────────────────────────────────────────────────
client.from_row::<Posts>().select("id").eq("status", "x").await?
  →  client.from_row::<Posts>().eq(Posts::status, "x".to_string()).execute().await?
  (or, to keep the old shape exactly:)
  →  client.from(Posts::TABLE).select("id").eq("status", "x").await?
  (or, to bridge mid-chain:)
  →  client.from_row::<Posts>().into_untyped().select("id").eq("status", "x").await?
```

#### Compile-time guarantees the typed path now enforces

- **Wrong table** — `client.from_row::<Posts>().eq(Users::id, ...)`
  fails to compile (the column is tied to a different `R`).
- **Wrong value type** — `client.from_row::<Posts>().eq(Posts::view_count, "abc")`
  fails to compile (`view_count` is `Column<Posts, i32>`).
- **`is_null` on a non-nullable column** — `is_null(Posts::status)`
  fails to compile when `status` is `NOT NULL`; the `is_null` signature
  requires `Column<R, Option<V>>`.
- **`like` on a non-string column** — `like(Posts::view_count, "10%")`
  fails to compile.

#### Notes

- The string-typed API (`client.from(table).eq("col", val)`) is
  unchanged. Existing code continues to work without modification.
- Runtime cost of `Column<R, V>` is zero — it's a `&'static str` plus
  a phantom type.
- `trybuild` compile-fail fixtures under `tests/trybuild/typed-columns/`
  codify each of the compile-time checks above.

## [0.4.0] - 2026-05-17

### Breaking — full removal of the pre-builder legacy API

The crate now exposes a single, supabase-js-style surface. Every method
previously marked `#[deprecated]` has been **deleted**. There is no
backward-compatibility shim; this is a hard cut.

#### Removed types and modules

- `crate::select::*` — `Operator`, `Filter`, `FilterGroup`,
  `LogicalOperator`, `Sort`, `SortDirection`, `SelectQuery`, `Field`,
  `Query`, `Condition`, and the `query!` macro.
- `crate::count` — module deleted.
- `crate::get`, `crate::insert`, `crate::update`, `crate::delete` —
  modules deleted.
- `crate::auth::users` — `SupabaseUser`, the legacy admin user helpers.
- `crate::auth::recover` — `forgot_password`, `reset_password`.
- `crate::auth::SignUpRequest`, `crate::auth::AuthResponse` —
  legacy payload structs.

#### Removed `SupabaseClient` methods

- `select`, `count`, `get_by_id`, `insert`, `upsert` (legacy),
  `update` (legacy two-arg form), `delete` (legacy two-arg form).
- `sign_up(SignUpRequest)`, `sign_in(email, password)`,
  `get_user(token)`, `delete_user(user_id)`,
  `get_all_users`, `get_user_by_id`,
  `forgot_password`, `reset_password`.

#### Migration map

```text
old                                              →  new
─────────────────────────────────────────────────────────────────────────────
client.select(table, SelectQuery{...})           →  client.from(table).select("*").eq(...).await
client.get_by_id(table, id)                      →  client.from(table).select("*").eq("id", id).single().await
client.insert(table, body)                       →  client.from(table).insert(body).select_returning("*").await
client.upsert(table, body)                       →  client.from(table).upsert(body).on_conflict("id").await
client.update(table, id, body)                   →  client.from(table).update(body).eq("id", id).await
client.delete(table, id)                         →  client.from(table).delete().eq("id", id).await
client.count(table, query)                       →  client.from(table).select("*").count(CountMode::Exact).execute_with_count().await
client.rpc_call(name, args)                      →  client.rpc_call(name, args)   (unchanged)
client.sign_up(SignUpRequest{...})               →  client.auth().sign_up(email, password, SignUpOptions{...}).await
client.sign_in(email, password)                  →  client.auth().sign_in_with_password(email, password).await
client.get_user(token)                           →  client.auth_with_token(token).get_user().await
client.delete_user(id)                           →  client.auth().admin().delete_user(id, false).await
client.get_all_users()                           →  client.auth().admin().list_users(1, 50).await
client.get_user_by_id(id)                        →  client.auth().admin().get_user_by_id(id).await
client.forgot_password(email)                    →  client.auth().reset_password_for_email(email, ResetPasswordOptions::default()).await
client.reset_password(pw, tok, code)             →  client.auth().verify_otp(VerifyOtpParams::TokenHash{...}).await
                                                     then client.auth().update_user(UpdateUserAttributes{password: Some(pw), ..}).await
```

#### Why

The legacy DSL (`SelectQuery` + `FilterGroup` + the `query!` macro) was
the original 0.1–0.2 surface. The PostgREST builder added in 0.3 covers
every operator the old DSL supported plus everything `supabase-js` exposes
(`maybe_single`, `single`, `range`, `text_search`, `not`, `or`,
foreign-table ordering, `count`, `returns::<T>()`, typed deserialization,
upsert with `on_conflict`, prefer headers, etc.). Maintaining two parallel
surfaces doubled the API documentation, doubled the test matrix, and
created a forking decision for every new user. Removing it leaves one
canonical path: `client.from(...).<builder>.<await>`.

#### Test suite changes
- Deleted `tests/select_tests.rs`, `tests/filter_tests.rs`,
  `tests/proptest_encoding.rs` (tested the deleted DSL types).
- Rewrote `tests/basic_tests.rs` against the builder API
  (`from(...).insert(...).select_returning(...)` etc.).
- Pruned legacy wrapper tests from the mock suite; renamed
  `legacy_and_remaining_mock_tests.rs` → `mock_coverage_tests.rs`.
- Test count: **611 → 510** (101 fewer, because each legacy method had
  duplicate tests covering the same wire-level behaviour as its
  supabase-js-style replacement).

## [0.3.0] - 2026-05-17

First release since 0.2.17. Bundles all unreleased work — foundation hardening,
realtime, edge functions, type-safety/DX polish, the coverage backfill, and CI.

### Phase 9 — Coverage-driven test backfill

A `cargo llvm-cov` audit revealed several modules sitting at 0% (legacy
`rpc`, `count`, `auth/users`, `auth/recover`) and a long tail of unhit
branches across PostgREST builder, Storage signed URLs, edge functions,
and the auth namespace. This pass closes them.

#### Coverage
- **Baseline:** 84.97% region / 84.13% line / 85.33% function
- **After:** **95.64% region / 96.95% line / 96.72% function**

Files reaching **100%** coverage: `auth/oauth.rs`, `auth/session_store.rs`,
`lib.rs`, `postgrest/filters.rs`, `postgrest/mod.rs`, `postgrest/row.rs`,
`postgrest/value.rs`, `rpc.rs`.

#### Added
- **`tests/legacy_and_remaining_mock_tests.rs` (94 tests)** —
  wiremock-based deterministic coverage for: legacy `rpc_call`, `count`
  (Content-Range parsing — happy, missing, malformed, unparseable),
  legacy `get_all_users`, `get_user_by_id`, `forgot_password`,
  `reset_password`, every `Functions::invoke*` variant (JSON, text, bytes
  with region, form, empty, custom HTTP method, 5xx, malformed,
  streaming success + error), the full auth namespace (`sign_up` with
  all options, `verify_otp` (Email/Phone/TokenHash), `resend`,
  `sign_in_with_otp`, `sign_in_with_id_token` with/without nonce,
  `exchange_code_for_session`, `reset_password_for_email`,
  `refresh_session` happy/error/edge paths, `refresh_session_if_needed`,
  `sign_out` (Local/Global/Others), `sign_in_with_oauth` URL building,
  phone-number password sign-in, decode-error paths on `get_user` and
  `update_user`, parse_session decode error), `request_bytes` retry +
  empty + malformed + 5xx paths, Storage `upload_to_signed_url` (success,
  error, empty body, malformed), `create_signed_url` with full transform
  options and `download` flag, batch `create_signed_urls` with absolute
  & relative rewriting, `create_signed_upload_url`, `download_response`
  success + error, `upload` with cache-control header, the admin namespace
  (`invite_user_by_email`, `generate_link` full + minimal,
  `list_users` pagination header parsing, empty body), and PostgREST
  builder execute-path variants (bare object, null, malformed JSON,
  array-element decode error, maybe_single multi-row,
  `IntoFuture` await on `single`/`maybe_single`).
- **`session_store.rs`** — poisoned-lock recovery tests for `get`/`set`/
  `clear` (the `PoisonError::into_inner()` branches).
- **`admin.rs`** — 8 unit tests for `parse_next_page` (next-rel
  extraction, missing rel, missing page, empty, unparseable) and
  `decode_user`.
- **`error.rs`** — `From<InvalidHeaderValue>`, every `SupabaseError`
  Display arm (`Storage`, `Url`, `InvalidHeader`, `Transport`,
  `Serialize`), `StorageError` Display without status, `AuthError`
  Display with status but no error_code.
- **`storage/mod.rs`** — `decode_json` success and failure unit tests.
- **`postgrest/builder.rs`** — `Order::default`, `Order::foreign_table`
  setter, `order_with` foreign-table emission, `serialize_body` failure
  path via a `Serialize` impl that always errors, `execute()` with a
  pre-set `body_error`, `Debug` impl smoke test.
- **`postgrest/mod.rs`** — `from_row<R: Row>` unit test.
- **`postgrest/filters.rs`** — `match_` rendering non-string values
  (numbers, bools), `match_` ignoring non-object input.
- **`select.rs`** — `SelectQuery::default` parity with `new`.
- **`cargo-supabase/main.rs`** — 6 additional `parse_args` tests
  covering `--schema`, both help forms, `--output` long form, empty
  argv defaults, "flag missing value" error path, `--apikey` alone.

#### Fixed
- **`PostgrestBuilder::build_path`** is now `pub` (was `pub(crate)`)
  so external test code can inspect request shape without sending.
- **Live storage test eventual consistency:** raised the
  `empty_bucket → delete_bucket` retry budget to 12 s (60 × 200 ms) to
  absorb tail latency observed in CI runs.

#### Notes on remaining gaps
The 4–5% still uncovered breaks down as:
- `cargo-supabase/src/main.rs` (54%): the async `main`/`fetch_openapi`
  entry points would require subprocess invocation tests. The pure
  `parse_args` is fully covered.
- A handful of `_ => panic!("wrong variant")` arms in unit tests —
  intentionally unreachable assertions.
- `SupabaseError::RetryExhausted` synthesis path in `send` and
  `request_bytes` — currently dead code because the loop returns the
  final 429 as a service error before falling through. A documented
  behaviour question, not a coverage one.

### Phase 8 — Production-grade test coverage

The unit-test foundation from Phase 7 is now backed by full integration
coverage of every public API surface. Total test count: **474** (278 unit +
57 codegen + 6 doc + 133 integration).

#### Added
- **Property-based encoding tests** (`tests/proptest_encoding.rs`, 14 tests).
  `proptest` is now a dev-dependency. Properties verified: filter values
  round-trip through `urlencoding::decode`, `SelectQuery::to_query_string`
  never produces `&&` / trailing `&` / raw CR/LF, `PostgrestBuilder::eq`
  round-trips adversarial inputs, chained filters preserve param counts,
  `or()` always wraps in encoded parens, `in_()` preserves list size,
  `order()` emits stable params, `limit`/`offset` are well-formed.
- **Concurrent session store stress tests**
  (`tests/session_store_concurrent.rs`, 5 tests). 8 readers + 4 writers
  + 2 clearers hammer `InMemorySessionStore` simultaneously; readers verify
  a session-internal invariant to detect torn reads. Also tests
  poisoned-lock recovery, last-writer-wins, snapshot independence, and that
  readers don't serialize (32 threads × 1000 reads in <2 s).
- **Mock-server transport tests** (`tests/mock_server_tests.rs`, 20 tests).
  `wiremock` is now a dev-dependency. Covers: 429 retry-with-backoff,
  exponential backoff timing, retry budget exhaustion, no-retry on 4xx/5xx,
  `apikey` + bearer header passthrough, schema headers (`Accept-Profile`),
  custom `with_access_token` bearer, builder `header()` and `user_agent()`
  passthrough, empty-body 200 returns `Vec<_>::new()`, malformed JSON →
  `SupabaseError::Decode`, service-aware error decoding (auth/postgrest),
  unique-constraint violations decoded as structured `PostgrestError`,
  RLS-denied requests, INSERT/UPDATE/PATCH/DELETE method routing, upsert
  `Prefer` header.
- **PostgREST builder integration tests** (`tests/postgrest_builder_tests.rs`,
  29 tests) against the live project. Covers every filter
  (`eq`/`neq`/`gt`/`gte`/`lt`/`lte`/`like`/`ilike`/`in`/`is`/`not`/`or`/`match_`),
  modifiers (`order`/`limit`/`offset`/`range`/`count`/`single`/`maybe_single`),
  CRUD (`insert`/`update`/`upsert`/`delete`), typed deserialization via
  `.returns::<T>()`, the `IntoFuture` await syntax, and edge cases like
  empty results and special characters in filter values. Every test is
  isolated by UUID-tagged `id1` for parallel-safe execution.
- **Auth integration tests** (`tests/auth_tests.rs`, 16 tests). Rewritten
  from a mostly-commented-out skeleton. Each test creates a confirmed user
  via the admin API and deletes it on exit, so no project-wide email-
  confirmation flag changes are required. Covers `sign_in_with_password`
  (success + wrong password + unknown email), `get_user`,
  `update_user`, `refresh_session` (success + invalid token),
  `sign_out` (Local + Global with server-side revocation check),
  `set_session`/`clear_session`, anonymous sign-in (gracefully skipped if
  not enabled), and the full admin CRUD + pagination surface.
- **Storage integration tests** (`tests/storage_tests.rs`, 18 tests). Every
  test creates its own UUID-named bucket and deletes it on exit. Covers
  bucket CRUD (create/get/update/delete + duplicate-name and missing-bucket
  errors), object upload/download/move/copy/remove, upsert semantics,
  `update` (PUT-replace), `list` with sort/limit, signed URLs (single +
  batch), public URL construction, and empty-bucket + delete sequencing
  under storage's eventual consistency.
- **Error-path integration tests** (`tests/error_path_tests.rs`, 13 tests).
  Live verification that the SDK surfaces the right error variants:
  PostgREST 404/400/42703 paths, `.single()` zero-row → `NotFound`, bogus
  operators, malformed JWTs, anon-key hitting admin endpoints, invalid API
  keys, unreachable URLs (connect-refused → `SupabaseError::Transport`),
  bucket-not-found, and post-deletion access.

#### Fixed
- **GoTrue `identities: null` deserialization bug.** Some users (notably
  freshly-created admin-side accounts) come back from
  `/auth/v1/admin/users` with `"identities": null`. Previously the
  `#[serde(default)]` on `Vec<Identity>` only handled *missing* fields, not
  null values, causing `SupabaseError::Decode { message: "invalid type:
  null, expected a sequence" }`. Added a `null_to_default` deserializer
  that maps null → `Vec::new()`.

#### Changed
- `PostgrestBuilder::build_path` is now `pub` (was `pub(crate)`). Useful
  for users who want to inspect or log the exact URL the builder will hit
  without sending a request — and lets external test code verify
  request shape.

### Phase 7 — Comprehensive testing & CI

#### Added
- **267 unit tests** across all modules (up from ~30): PostgREST builder (all
  filter methods, `order_with`, `parse_content_range`, `match_`, `count`
  modes), value encoding, filter primitives, error types (`SupabaseError`
  variants, Display, `decode_error` routing, `From` conversions), Realtime
  event types (all enum variants, `PostgresChangesFilter` serialization,
  payload deserialization), auth types (`Session` expiry math,
  `SignOutScope`, `OAuthProvider`, `Identity`), storage types (all bucket /
  upload / list / signed-URL structures), bucket API (`encode_path`,
  `get_public_url`, `decode_json`, `object_path`), Edge Functions (`InvokeBody`,
  `InvokeMethod`, `FunctionRegion` — all 15 named regions — `InvokeOptions`
  builder, form encoding), codegen CLI (all 15+ PostgreSQL→Rust type
  mappings, `to_struct_name`, `to_field_name`, `is_rust_keyword`, `emit`
  edge cases, `OpenApi` deserialization).
- `SignOutScope` now derives `Serialize` + `Deserialize` with
  `#[serde(rename_all = "lowercase")]`, enabling direct JSON serialization
  where needed.
- **GitHub Actions CI workflow** (`.github/workflows/ci.yml`) with jobs:
  - `fmt` — `cargo fmt --all -- --check`
  - `clippy` — `--all-features` and `--no-default-features`
  - `test` — matrix over 8 feature combinations (`--all-features`,
    individual features, `--no-default-features`)
  - `test-cli` — `cargo-supabase` test suite
  - `examples` — `cargo build --examples --all-features`
  - `msrv` — build + test on Rust 1.75 (declared MSRV)
  - `tls-backends` — compile with `rustls` and `native-tls`
- Codegen bug-fix: array-of-text columns (`Vec<String>`) now resolve
  correctly; previously the `kind = "array"` pass-through defeated the
  string-type guard in `map_scalar_format`.

### Phase 6 — Type safety & DX polish

#### Added
- `rust_supabase_sdk::Row` trait — implementors carry their PostgREST
  `TABLE` (and optional `SCHEMA` / `COLUMNS` metadata) at the type level.
- `SupabaseClient::from_row::<R>()` typed table accessor — same return type
  as `from(&str)`, but the table name comes from the `Row` impl.
- **`cargo-supabase` CLI** (new workspace member). Run
  `cargo supabase gen types --url $SUPABASE_URL --apikey $SUPABASE_API_KEY`
  to introspect the project's PostgREST OpenAPI document and emit a Rust
  module of row structs + `Row` impls. Flags: `--schema`, `--output`,
  `--only`, `--exclude`, `--no-chrono`, `--uuid`.
- `examples/postgrest_typed.rs` showing the typed query path.

#### Changed
- Repo restructured as a Cargo workspace. The root crate is still
  `rust_supabase_sdk`; the new `cargo-supabase` crate lives at
  `./cargo-supabase`.

### Phase 5 — Edge Functions

#### Added
- `client.functions()` namespace gated on the `functions` feature (enabled by
  default — no extra dependencies).
- `Functions::invoke<Req, Res>(name, &body)` JSON fast-path mirroring
  supabase-js's `functions.invoke(name, { body })`.
- `Functions::invoke_with<Res>(name, options)` taking full
  [`InvokeOptions`] (body, headers, region, method).
- `Functions::invoke_stream(name, options)` returning a raw
  [`reqwest::Response`] for streaming responses (SSE, large payloads).
- `InvokeBody` covering `Json` / `Bytes` / `Text` / `Form` / `Empty` with
  automatic `Content-Type`.
- `FunctionRegion` enum (15 canonical AWS regions + `Custom(String)`),
  forwarded via the `x-region` header.
- `InvokeMethod` with `Post` as the default, matching supabase-js.
- `examples/functions_invoke.rs` demonstrating all three call shapes.

#### Changed
- Default features now include `functions` alongside `postgrest`/`auth`/`storage`.

### Phase 4 — Realtime

#### Added
- `client.realtime()` namespace gated on the `realtime` feature. Backed by a
  single multiplexed WebSocket (Phoenix Channels protocol) with periodic
  heartbeats.
- `RealtimeClient::channel(topic)` returning a [`ChannelBuilder`]
  (`on_postgres_changes` / `on_broadcast` / `on_presence` for stream-style use,
  plus `on_postgres_changes_callback` / `on_broadcast_callback` /
  `on_presence_callback` for supabase-js-style callbacks).
- `Channel` exposes both `recv` (mpsc-style) and `impl futures_util::Stream`
  for use with `StreamExt::next` / `while let Some(...)`.
- `Channel::run().await` pumps events through registered callbacks until the
  channel closes.
- `RealtimeClient::set_auth(token)` / `Channel::set_auth(token)` for refreshing
  the JWT on already-joined channels.
- Automatic reconnect with exponential backoff (`ReconnectPolicy`), capped at
  30s. Joined channels replay their subscriptions transparently on reconnect.
- `Realtime::reconnect(policy)` / `Realtime::no_reconnect()` builder hooks.
- `SubscriptionStatus`, `PresenceEvent`, and `PostgresChangeKind::matches`
  utilities.
- `examples/realtime_changes.rs` demonstrating both stream and callback styles.

#### Changed
- Internal connection actor split into a supervisor task that owns the live
  WebSocket and rebuilds it on disconnect, plus a heartbeat task.

### Phase 1 — Foundation hardening

#### Added
- Cargo feature flags: `postgrest`, `auth`, `storage`, `realtime`, `functions`,
  `rustls`, `native-tls` (default = postgrest+auth+storage+rustls).
- `thiserror`-based `SupabaseError` with `#[non_exhaustive]` and a new
  `Serialize(serde_json::Error)` variant.
- `tracing` integration — `debug!` per request, `warn!` on 429 retries.
- `ClientBuilder::timeout`, `::max_retries`, `::user_agent`, `::retry`.
- `RetryConfig` exposed publicly; client-level retry policy replaces the
  hard-coded `MAX_RETRIES = 5`.
- Body-serialization failure in `from(t).insert/.upsert/.update(body)` is now
  surfaced at `.execute()` time instead of silently sending `null`.

#### Changed
- `reqwest` pinned to `0.13.3`, `default-features = false`; pulls in
  `rustls` (or `native-tls`) via SDK feature.
- `tokio` reduced from `features = ["full"]` to `rt + macros + time`.
- `dotenv` moved to `dev-dependencies`. `rand` removed.
- `chrono` reduced to `serde + clock`.

#### Removed
- `rand` runtime dep (was unused).

### Migration

The public API for the new namespaces (`client.auth()`, `client.from()`,
`client.storage()`) is unchanged. The legacy top-level methods on
`SupabaseClient` remain `#[deprecated]` and will be removed in 0.7.0.

If you import the crate with default features, no changes are required. If you
build with `default-features = false`, opt back into the modules you use.

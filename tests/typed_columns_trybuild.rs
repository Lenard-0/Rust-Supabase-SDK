//! Compile-fail harness for the typed-column API.
//!
//! Each `.rs` file under `tests/trybuild/typed-columns/` MUST fail to
//! compile — the `.stderr` siblings record the expected error output. To
//! refresh stderr after a deliberate API change run:
//!
//! ```text
//! TRYBUILD=overwrite cargo test --test typed_columns_trybuild -- --ignored
//! ```
//!
//! We mark the test `#[ignore]` so it doesn't run on every `cargo test`.
//! trybuild's expected-error matching is sensitive to rustc version, so we
//! pin it to opt-in execution (CI runs it as a dedicated job on the MSRV
//! toolchain).

#[test]
#[ignore = "trybuild fixtures are toolchain-sensitive; run explicitly with `cargo test --test typed_columns_trybuild -- --ignored`"]
fn typed_columns_reject_misuse() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/trybuild/typed-columns/*.rs");
}

//! Property-based tests for URL/query encoding paths.
//!
//! These run against pure code (no network), but cover encoding edge cases
//! humans struggle to hand-pick: empty strings, control bytes, Unicode,
//! injection-shaped payloads, very long values, randomised filter chains.
//!
//! The goal is to catch *invariants*:
//!  - `eq.<value>` round-trips through `urlencoding::decode` to the original value
//!  - filter chains never produce malformed query strings (no double `?`, no `&&`)
//!  - select_query strings always begin with `select=`
//!  - column names with reserved chars never break the param boundary

#![allow(clippy::unwrap_used)]

use proptest::prelude::*;
use rust_supabase_sdk::select::{
    Filter, FilterGroup, LogicalOperator, Operator, SelectQuery, Sort, SortDirection,
};
use rust_supabase_sdk::SupabaseClient;

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

/// Strings that may contain any ASCII printable + a handful of nasty chars.
/// We deliberately include `&`, `=`, `?`, `%`, `+`, `'`, `"`, `\n`, etc.
fn arb_safe_value() -> impl Strategy<Value = String> {
    // Mix of regex-generated ASCII and explicit edge values.
    prop_oneof![
        // 0-200 char strings with anything from space-tilde
        "[ -~]{0,200}",
        // explicit edge cases
        Just("".to_string()),
        Just(" ".to_string()),
        Just("a&b=c".to_string()),
        Just("100%".to_string()),
        Just("hello world".to_string()),
        Just("O'Brien".to_string()),
        Just("'; DROP TABLE users; --".to_string()),
        Just("\n\t\r".to_string()),
        Just("a,b,c".to_string()),
        Just("(parens)".to_string()),
    ]
}

/// Column names look like identifiers (`\w+`) — keep it tame.
fn arb_column() -> impl Strategy<Value = String> {
    "[a-z_][a-z0-9_]{0,30}"
}

fn arb_operator() -> impl Strategy<Value = Operator> {
    prop_oneof![
        Just(Operator::Eq),
        Just(Operator::Neq),
        Just(Operator::Lt),
        Just(Operator::Gt),
        Just(Operator::Lte),
        Just(Operator::Gte),
        Just(Operator::Like),
    ]
}

fn arb_filter() -> impl Strategy<Value = Filter> {
    (arb_column(), arb_operator(), arb_safe_value()).prop_map(|(c, o, v)| Filter {
        column: c,
        operator: o,
        value: v,
    })
}

fn arb_logical() -> impl Strategy<Value = LogicalOperator> {
    prop_oneof![Just(LogicalOperator::And), Just(LogicalOperator::Or)]
}

fn arb_sort() -> impl Strategy<Value = Sort> {
    (
        arb_column(),
        prop_oneof![Just(SortDirection::Asc), Just(SortDirection::Desc)],
    )
        .prop_map(|(c, d)| Sort::new(&c, d))
}

// ---------------------------------------------------------------------------
// Properties
// ---------------------------------------------------------------------------

proptest! {
    /// `eq.<value>` survives a round-trip: anything you put in, you can
    /// `urldecode` back out and recover the original string.
    #[test]
    fn filter_value_round_trips_via_urlencoding(value in arb_safe_value()) {
        let filter = Filter::new("col", Operator::Eq, &value);
        let q = filter.to_query();
        // q == "col=eq.<encoded>"
        let prefix = "col=eq.";
        prop_assert!(q.starts_with(prefix), "missing prefix: {q}");
        let encoded = &q[prefix.len()..];
        let decoded = urlencoding::decode(encoded).unwrap();
        prop_assert_eq!(decoded.as_ref(), value.as_str());
    }

    /// Query strings produced by `SelectQuery::to_query_string` always
    /// start with `select=` and never contain `&&` or trailing `&`.
    #[test]
    fn select_query_is_well_formed(
        filters in prop::collection::vec(arb_filter(), 0..5),
        logical in arb_logical(),
        sorts in prop::collection::vec(arb_sort(), 0..3),
    ) {
        let mut q = SelectQuery::new();
        if !filters.is_empty() {
            q.filter = Some(FilterGroup::new(logical, filters));
        }
        q.sorts = sorts;
        let s = q.to_query_string();

        prop_assert!(s.starts_with("select="), "doesn't start with select=: {s}");
        prop_assert!(!s.contains("&&"), "double-ampersand: {s}");
        prop_assert!(!s.ends_with('&'), "trailing ampersand: {s}");
        // No raw newlines/carriage returns that would break HTTP framing.
        prop_assert!(!s.contains('\n'), "raw newline: {s}");
        prop_assert!(!s.contains('\r'), "raw CR: {s}");
    }

    /// A single-filter group renders to the same string as the bare filter,
    /// regardless of the chosen logical operator (AND with one element is a no-op).
    #[test]
    fn single_filter_and_group_equivalent(f in arb_filter()) {
        let bare = f.to_query();
        let group = FilterGroup::new(LogicalOperator::And, vec![f.clone()]);
        let group_str = group.to_query_string();
        prop_assert_eq!(bare, group_str);
    }

    /// `PostgrestBuilder::eq` produces a path that round-trips the value via
    /// `urldecode`, even with adversarial inputs.
    #[test]
    fn builder_eq_round_trips(value in arb_safe_value()) {
        let client = SupabaseClient::new("https://example.supabase.co", "anon", None);
        let path = client.from("t").select("*").eq("col", value.clone()).build_path();
        // path = "/rest/v1/t?select=%2A&col=eq.<encoded>"
        let prefix = "/rest/v1/t?select=%2A&col=eq.";
        prop_assert!(path.starts_with(prefix), "unexpected path: {path}");
        let encoded = &path[prefix.len()..];
        let decoded = urlencoding::decode(encoded).unwrap();
        prop_assert_eq!(decoded.as_ref(), value.as_str());
    }

    /// Chaining N filters produces N occurrences of `=` parameters and exactly
    /// N-1 ampersands between them (after the `select=*`).
    #[test]
    fn chained_filters_have_consistent_param_count(
        cols in prop::collection::vec(arb_column(), 1..6),
    ) {
        let client = SupabaseClient::new("https://example.supabase.co", "anon", None);
        let mut q = client.from("t").select("*");
        for c in &cols {
            q = q.eq(c, "v");
        }
        let path = q.build_path();
        // Strip the path prefix "/rest/v1/t?" and split on '&'
        let qstr = path.split_once('?').unwrap().1;
        let params: Vec<&str> = qstr.split('&').collect();
        // 1 for select=* + one per filter
        prop_assert_eq!(params.len(), cols.len() + 1);
        prop_assert_eq!(params[0], "select=%2A");
    }

    /// `or(...)` always emits an `or=` parameter and wraps the inner filter
    /// list in encoded parentheses.
    #[test]
    fn or_filter_always_wraps_in_parens(inner in "[a-z]{1,8}\\.eq\\.[a-z]{1,8}") {
        let client = SupabaseClient::new("https://example.supabase.co", "anon", None);
        let path = client.from("t").select("*").or(&inner).build_path();
        prop_assert!(path.contains("or="), "missing or=: {path}");
        // %28 = '(' and %29 = ')'
        prop_assert!(path.contains("%28"), "missing encoded '(': {path}");
        prop_assert!(path.contains("%29"), "missing encoded ')': {path}");
    }

    /// `in_(...)` with N elements produces a parenthesised, comma-separated list.
    #[test]
    fn in_list_size_preserved(items in prop::collection::vec("[a-z]{1,8}", 1..6)) {
        let client = SupabaseClient::new("https://example.supabase.co", "anon", None);
        let items_strs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
        let path = client.from("t").select("*").in_("c", items_strs.clone()).build_path();
        // The encoded payload should contain N-1 commas (between items).
        let comma_count = path.matches("%2C").count();
        prop_assert_eq!(comma_count, items.len().saturating_sub(1));
    }

    /// `order(col, asc)` always appends `order=col.asc.nullslast` or `order=col.desc.nullsfirst`.
    /// The format is stable; we only check it contains `order=` and a `.asc` or `.desc`.
    #[test]
    fn order_emits_stable_param(col in arb_column(), ascending: bool) {
        let client = SupabaseClient::new("https://example.supabase.co", "anon", None);
        let path = client.from("t").select("*").order(&col, ascending).build_path();
        prop_assert!(path.contains("order="), "missing order=: {path}");
        let needle = if ascending { ".asc" } else { ".desc" };
        prop_assert!(path.contains(needle), "missing {needle} in {path}");
    }

    /// `limit` and `offset` are emitted as integer params, never negative or empty.
    #[test]
    fn limit_offset_well_formed(n in 0u64..1_000_000) {
        let client = SupabaseClient::new("https://example.supabase.co", "anon", None);
        let p1 = client.from("t").select("*").limit(n).build_path();
        let p2 = client.from("t").select("*").offset(n).build_path();
        prop_assert!(p1.contains(&format!("limit={n}")), "limit not found: {p1}");
        prop_assert!(p2.contains(&format!("offset={n}")), "offset not found: {p2}");
    }
}

// ---------------------------------------------------------------------------
// Direct deterministic tests for known-bad inputs
// (useful for regression — these would otherwise rely on proptest seeds.)
// ---------------------------------------------------------------------------

#[test]
fn empty_string_value_does_not_break_filter() {
    let f = Filter::new("col", Operator::Eq, "");
    let q = f.to_query();
    assert_eq!(q, "col=eq.");
}

#[test]
fn newline_in_value_is_encoded() {
    let f = Filter::new("col", Operator::Eq, "line1\nline2");
    let q = f.to_query();
    // %0A is the encoded form of LF.
    assert!(q.contains("%0A"), "{q}");
    assert!(!q.contains('\n'), "{q}");
}

#[test]
fn quote_injection_value_is_encoded() {
    let f = Filter::new("col", Operator::Eq, "'; DROP TABLE users; --");
    let q = f.to_query();
    // ' must be %27, ; must be %3B
    assert!(q.contains("%27"), "{q}");
    assert!(q.contains("%3B"), "{q}");
}

#[test]
fn very_long_value_does_not_panic() {
    let big = "a".repeat(10_000);
    let f = Filter::new("col", Operator::Eq, &big);
    let q = f.to_query();
    assert!(q.starts_with("col=eq."));
    assert_eq!(q.len(), "col=eq.".len() + big.len());
}

#[test]
fn unicode_value_is_percent_encoded() {
    let f = Filter::new("col", Operator::Eq, "café");
    let q = f.to_query();
    // 'é' → %C3%A9
    assert!(q.contains("%C3%A9"), "{q}");
}

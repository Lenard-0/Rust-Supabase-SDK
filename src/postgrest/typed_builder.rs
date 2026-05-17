//! Type-safe query builder for codegen-emitted row types.
//!
//! Returned by [`SupabaseClient::from_row::<R>()`](crate::SupabaseClient::from_row).
//! Wraps the untyped [`PostgrestBuilder`] / [`TableBuilder`] surface, adding
//! compile-time checks for:
//!
//!   * column ownership — `Posts::status` can only be used inside a
//!     `TypedBuilder<Posts>`; passing it to a `TypedBuilder<Users>` is a
//!     compile error,
//!   * value-type match — `eq(Posts::view_count, "abc")` is a compile error
//!     because `view_count` is `Column<Posts, i32>`,
//!   * nullability — `is_null` only accepts `Column<R, Option<V>>`,
//!   * `like`/`ilike` only accept string-typed columns.
//!
//! Internally this just funnels through [`PostgrestBuilder<Value>`] — the
//! type-safety layer is purely at the front door. Anything you can't express
//! through the typed API still works via the untyped
//! [`SupabaseClient::from`](crate::SupabaseClient::from) path.

use std::future::{Future, IntoFuture};
use std::pin::Pin;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use crate::error::Result;

use super::builder::{
    CountMode, MaybeSingleBuilder, Order, PostgrestBuilder, SingleBuilder, TableBuilder,
    TextSearchType,
};
use super::column::{Column, IntoColumnName};
use super::row::Row;

/// Type-safe PostgREST query builder bound to row type `R`.
///
/// Construct via [`SupabaseClient::from_row::<R>()`](crate::SupabaseClient::from_row).
#[must_use = "TypedBuilder is lazy — `.await` it or call `.execute()`"]
pub struct TypedBuilder<R: Row> {
    inner: PostgrestBuilder<Value>,
    _row: std::marker::PhantomData<fn() -> R>,
}

impl<R: Row> TypedBuilder<R> {
    pub(crate) fn from_table(table: TableBuilder) -> Self {
        Self {
            inner: table.select("*"),
            _row: std::marker::PhantomData,
        }
    }

    /// Replace the column projection. `columns` follows PostgREST syntax
    /// (`"id,name"`, `"*"`, or `"id,name,profile(*)"`). Punted from the
    /// initial typed-columns work — wrong column names here just produce
    /// missing JSON keys, not corrupt state.
    pub fn select(mut self, columns: impl Into<String>) -> Self {
        self.inner = self.inner.select_returning(columns);
        self
    }

    // -----------------------------------------------------------------
    // Filter methods — value type tied to the column's declared type.
    // -----------------------------------------------------------------

    /// `column = value` filter.
    pub fn eq<V>(mut self, col: Column<R, V>, val: V) -> Self
    where
        V: Serialize + std::fmt::Display,
    {
        self.inner = self.inner.eq(col.name(), val.to_string());
        self
    }

    /// `column <> value` filter.
    pub fn neq<V>(mut self, col: Column<R, V>, val: V) -> Self
    where
        V: Serialize + std::fmt::Display,
    {
        self.inner = self.inner.neq(col.name(), val.to_string());
        self
    }

    /// `column > value` filter.
    pub fn gt<V>(mut self, col: Column<R, V>, val: V) -> Self
    where
        V: Serialize + std::fmt::Display + PartialOrd,
    {
        self.inner = self.inner.gt(col.name(), val.to_string());
        self
    }

    /// `column >= value` filter.
    pub fn gte<V>(mut self, col: Column<R, V>, val: V) -> Self
    where
        V: Serialize + std::fmt::Display + PartialOrd,
    {
        self.inner = self.inner.gte(col.name(), val.to_string());
        self
    }

    /// `column < value` filter.
    pub fn lt<V>(mut self, col: Column<R, V>, val: V) -> Self
    where
        V: Serialize + std::fmt::Display + PartialOrd,
    {
        self.inner = self.inner.lt(col.name(), val.to_string());
        self
    }

    /// `column <= value` filter.
    pub fn lte<V>(mut self, col: Column<R, V>, val: V) -> Self
    where
        V: Serialize + std::fmt::Display + PartialOrd,
    {
        self.inner = self.inner.lte(col.name(), val.to_string());
        self
    }

    /// SQL `LIKE` pattern match. Only valid on string-typed columns.
    pub fn like(mut self, col: Column<R, String>, pattern: impl AsRef<str>) -> Self {
        self.inner = self.inner.like(col.name(), pattern.as_ref());
        self
    }

    /// SQL `ILIKE` (case-insensitive) pattern match. String columns only.
    pub fn ilike(mut self, col: Column<R, String>, pattern: impl AsRef<str>) -> Self {
        self.inner = self.inner.ilike(col.name(), pattern.as_ref());
        self
    }

    /// `column IS NULL`. Only allowed on nullable columns — `is_null` on a
    /// `NOT NULL` column is always a bug, so the compiler rejects it.
    pub fn is_null<V>(mut self, col: Column<R, Option<V>>) -> Self {
        self.inner = self.inner.is(col.name(), "null");
        self
    }

    /// `column IS NOT NULL`. Same nullability constraint as [`is_null`].
    ///
    /// [`is_null`]: TypedBuilder::is_null
    pub fn is_not_null<V>(mut self, col: Column<R, Option<V>>) -> Self {
        self.inner = self.inner.is(col.name(), "not.null");
        self
    }

    /// `column IS TRUE` / `IS FALSE` — only valid on boolean columns.
    pub fn is_bool(mut self, col: Column<R, bool>, value: bool) -> Self {
        self.inner = self
            .inner
            .is(col.name(), if value { "true" } else { "false" });
        self
    }

    /// `column = ANY (vals)` — value type matches the column.
    pub fn in_<V, I>(mut self, col: Column<R, V>, vals: I) -> Self
    where
        V: Serialize + std::fmt::Display,
        I: IntoIterator<Item = V>,
    {
        self.inner = self
            .inner
            .in_(col.name(), vals.into_iter().map(|v| v.to_string()));
        self
    }

    /// `column @> value` — array/jsonb containment.
    pub fn contains<V>(mut self, col: Column<R, V>, val: V) -> Self
    where
        V: Serialize + std::fmt::Display,
    {
        self.inner = self.inner.contains(col.name(), val.to_string());
        self
    }

    /// `column <@ value` — contained-by.
    pub fn contained_by<V>(mut self, col: Column<R, V>, val: V) -> Self
    where
        V: Serialize + std::fmt::Display,
    {
        self.inner = self.inner.contained_by(col.name(), val.to_string());
        self
    }

    /// `column && value` — range/array overlap.
    pub fn overlaps<V>(mut self, col: Column<R, V>, val: V) -> Self
    where
        V: Serialize + std::fmt::Display,
    {
        self.inner = self.inner.overlaps(col.name(), val.to_string());
        self
    }

    // ---- Negation variants of the common filters ----

    /// `column <> value` (alias of [`neq`]).
    ///
    /// [`neq`]: TypedBuilder::neq
    pub fn not_eq<V>(self, col: Column<R, V>, val: V) -> Self
    where
        V: Serialize + std::fmt::Display,
    {
        self.neq(col, val)
    }

    /// `NOT (column = ANY (vals))`.
    pub fn not_in_<V, I>(mut self, col: Column<R, V>, vals: I) -> Self
    where
        V: Serialize + std::fmt::Display,
        I: IntoIterator<Item = V>,
    {
        let rendered: Vec<String> = vals.into_iter().map(|v| v.to_string()).collect();
        let raw = format!("({})", rendered.join(","));
        self.inner = self.inner.not(col.name(), "in", &raw);
        self
    }

    /// `NOT (column LIKE pattern)`. String columns only.
    pub fn not_like(mut self, col: Column<R, String>, pattern: impl AsRef<str>) -> Self {
        self.inner = self.inner.not(col.name(), "like", pattern.as_ref());
        self
    }

    /// `NOT (column ILIKE pattern)`. String columns only.
    pub fn not_ilike(mut self, col: Column<R, String>, pattern: impl AsRef<str>) -> Self {
        self.inner = self.inner.not(col.name(), "ilike", pattern.as_ref());
        self
    }

    // -----------------------------------------------------------------
    // Ordering / pagination — typed *or* string columns.
    // -----------------------------------------------------------------

    /// Order by a column. Accepts both [`Column<R, _>`] and `&str` so views
    /// or function-result columns can still be sorted on.
    pub fn order(mut self, col: impl IntoColumnName<R>, ascending: bool) -> Self {
        self.inner = self
            .inner
            .order(&col.into_column_name(), ascending);
        self
    }

    /// Order with full [`Order`] options (nulls-first/last, foreign-table).
    pub fn order_with(mut self, col: impl IntoColumnName<R>, options: Order) -> Self {
        self.inner = self
            .inner
            .order_with(&col.into_column_name(), options);
        self
    }

    /// Limit the number of returned rows.
    pub fn limit(mut self, n: u64) -> Self {
        self.inner = self.inner.limit(n);
        self
    }

    /// Skip the first `n` matching rows.
    pub fn offset(mut self, n: u64) -> Self {
        self.inner = self.inner.offset(n);
        self
    }

    /// Inclusive byte-style range `[from, to]`.
    pub fn range(mut self, from: u64, to: u64) -> Self {
        self.inner = self.inner.range(from, to);
        self
    }

    /// Attach a `Prefer: count=<mode>` header so the response carries a
    /// Content-Range total.
    pub fn count(mut self, mode: CountMode) -> Self {
        self.inner = self.inner.count(mode);
        self
    }

    // -----------------------------------------------------------------
    // Text search — string columns only.
    // -----------------------------------------------------------------

    /// `to_tsquery`-style full-text search on a string column.
    pub fn text_search(
        mut self,
        col: Column<R, String>,
        query: &str,
        kind: TextSearchType,
        config: Option<&str>,
    ) -> Self {
        self.inner = self.inner.text_search(col.name(), query, kind, config);
        self
    }

    // -----------------------------------------------------------------
    // Execution — same shapes as the untyped builder, but the default
    // typed-row hint is preserved.
    // -----------------------------------------------------------------

    /// Execute and deserialize each row into `R`. This is the typed path —
    /// no `.returns::<T>()` needed.
    pub async fn execute(self) -> Result<Vec<R>> {
        self.inner.returns::<R>().execute().await
    }

    /// Execute and return both `(rows, count)` — useful with [`count`].
    ///
    /// [`count`]: TypedBuilder::count
    pub async fn execute_with_count(self) -> Result<(Vec<R>, Option<u64>)> {
        self.inner
            .returns::<R>()
            .execute_with_count()
            .await
    }

    /// Expect exactly one row; error otherwise.
    pub fn single(self) -> SingleBuilder<R> {
        self.inner.returns::<R>().single()
    }

    /// Expect zero or one row.
    pub fn maybe_single(self) -> MaybeSingleBuilder<R> {
        self.inner.returns::<R>().maybe_single()
    }

    /// Override the return type — useful for `select("count")`-style
    /// projection queries that don't deserialize into `R`.
    pub fn returns<U>(self) -> PostgrestBuilder<U> {
        self.inner.returns::<U>()
    }

    /// Drop down to the untyped builder. Escape hatch when you need
    /// operations the typed builder doesn't expose.
    pub fn into_untyped(self) -> PostgrestBuilder<Value> {
        self.inner
    }

    /// Render the path that this builder would send. Useful for tests, logs,
    /// and debugging without firing a network request.
    pub fn build_path(&self) -> String {
        self.inner.build_path()
    }
}

impl<R: Row + DeserializeOwned + Send + 'static> IntoFuture for TypedBuilder<R> {
    type Output = Result<Vec<R>>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.execute())
    }
}

// `SupabaseClient::from_row::<R>()` lives in `postgrest/mod.rs` and returns
// `TypedBuilder<R>` via the `pub(crate) TypedBuilder::from_table` ctor.

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::SupabaseClient;
    use serde::{Deserialize, Serialize};

    // Fixture row type with one column of each interesting shape.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Posts {
        id: String,
        status: String,
        view_count: i32,
        archived: Option<bool>,
        published_at: Option<String>,
    }

    impl Row for Posts {
        const TABLE: &'static str = "posts";
    }

    #[allow(non_upper_case_globals)]
    impl Posts {
        pub const id: Column<Posts, String> = Column::new("id");
        pub const status: Column<Posts, String> = Column::new("status");
        pub const view_count: Column<Posts, i32> = Column::new("view_count");
        pub const archived: Column<Posts, Option<bool>> = Column::new("archived");
        pub const published_at: Column<Posts, Option<String>> = Column::new("published_at");
    }

    fn client() -> SupabaseClient {
        SupabaseClient::new("https://x.supabase.co", "anon", None)
    }

    // ---- construction ----

    #[test]
    fn from_row_defaults_to_select_star() {
        let p = client().from_row::<Posts>().build_path();
        assert_eq!(p, "/rest/v1/posts?select=%2A");
    }

    #[test]
    fn select_overrides_default_projection() {
        let p = client()
            .from_row::<Posts>()
            .select("id,status")
            .build_path();
        assert!(p.contains("select=id%2Cstatus"), "p={p}");
    }

    // ---- equality / inequality ----

    #[test]
    fn eq_uses_column_name() {
        let p = client()
            .from_row::<Posts>()
            .eq(Posts::status, "published".to_string())
            .build_path();
        assert!(p.contains("status=eq.published"), "p={p}");
    }

    #[test]
    fn neq_uses_column_name() {
        let p = client()
            .from_row::<Posts>()
            .neq(Posts::status, "draft".to_string())
            .build_path();
        assert!(p.contains("status=neq.draft"), "p={p}");
    }

    #[test]
    fn not_eq_aliases_neq() {
        let p = client()
            .from_row::<Posts>()
            .not_eq(Posts::status, "draft".to_string())
            .build_path();
        assert!(p.contains("status=neq.draft"), "p={p}");
    }

    // ---- ordering filters ----

    #[test]
    fn gt_emits_op() {
        let p = client()
            .from_row::<Posts>()
            .gt(Posts::view_count, 100i32)
            .build_path();
        assert!(p.contains("view_count=gt.100"), "p={p}");
    }

    #[test]
    fn gte_emits_op() {
        let p = client()
            .from_row::<Posts>()
            .gte(Posts::view_count, 100i32)
            .build_path();
        assert!(p.contains("view_count=gte.100"), "p={p}");
    }

    #[test]
    fn lt_emits_op() {
        let p = client()
            .from_row::<Posts>()
            .lt(Posts::view_count, 100i32)
            .build_path();
        assert!(p.contains("view_count=lt.100"), "p={p}");
    }

    #[test]
    fn lte_emits_op() {
        let p = client()
            .from_row::<Posts>()
            .lte(Posts::view_count, 100i32)
            .build_path();
        assert!(p.contains("view_count=lte.100"), "p={p}");
    }

    // ---- patterns ----

    #[test]
    fn like_pattern_emits_encoded_op() {
        let p = client()
            .from_row::<Posts>()
            .like(Posts::status, "pub%")
            .build_path();
        assert!(p.contains("status=like.pub%25"), "p={p}");
    }

    #[test]
    fn ilike_pattern_emits_encoded_op() {
        let p = client()
            .from_row::<Posts>()
            .ilike(Posts::status, "PUB%")
            .build_path();
        assert!(p.contains("status=ilike.PUB%25"), "p={p}");
    }

    #[test]
    fn not_like_pattern() {
        let p = client()
            .from_row::<Posts>()
            .not_like(Posts::status, "dra%")
            .build_path();
        assert!(p.contains("status=not.like."), "p={p}");
    }

    #[test]
    fn not_ilike_pattern() {
        let p = client()
            .from_row::<Posts>()
            .not_ilike(Posts::status, "dra%")
            .build_path();
        assert!(p.contains("status=not.ilike."), "p={p}");
    }

    // ---- nullability ----

    #[test]
    fn is_null_only_compiles_on_nullable_columns() {
        // `archived` is Option<bool> → allowed.
        let p = client()
            .from_row::<Posts>()
            .is_null(Posts::archived)
            .build_path();
        assert!(p.contains("archived=is.null"), "p={p}");
    }

    #[test]
    fn is_not_null_only_compiles_on_nullable_columns() {
        let p = client()
            .from_row::<Posts>()
            .is_not_null(Posts::published_at)
            .build_path();
        assert!(p.contains("published_at=is.not.null"), "p={p}");
    }

    #[test]
    fn is_bool_emits_true_or_false() {
        // Synthesize a Column<R, bool> for the test — fixture has none.
        let col: Column<Posts, bool> = Column::new("is_active");
        let p_true = client().from_row::<Posts>().is_bool(col, true).build_path();
        let p_false = client().from_row::<Posts>().is_bool(col, false).build_path();
        assert!(p_true.contains("is_active=is.true"), "p={p_true}");
        assert!(p_false.contains("is_active=is.false"), "p={p_false}");
    }

    // ---- list filters ----

    #[test]
    fn in_renders_paren_list() {
        let p = client()
            .from_row::<Posts>()
            .in_(Posts::status, ["a".to_string(), "b".to_string(), "c".to_string()])
            .build_path();
        assert!(p.contains("status=in."), "p={p}");
        assert!(p.contains("a") && p.contains("b") && p.contains("c"), "p={p}");
    }

    #[test]
    fn not_in_renders_negated_op() {
        let p = client()
            .from_row::<Posts>()
            .not_in_(Posts::status, ["x".to_string(), "y".to_string()])
            .build_path();
        assert!(p.contains("status=not.in"), "p={p}");
    }

    // ---- array/range ops ----

    #[test]
    fn contains_emits_cs_op() {
        let p = client()
            .from_row::<Posts>()
            .contains(Posts::status, "x".to_string())
            .build_path();
        assert!(p.contains("status=cs."), "p={p}");
    }

    #[test]
    fn contained_by_emits_cd_op() {
        let p = client()
            .from_row::<Posts>()
            .contained_by(Posts::status, "x".to_string())
            .build_path();
        assert!(p.contains("status=cd."), "p={p}");
    }

    #[test]
    fn overlaps_emits_ov_op() {
        let p = client()
            .from_row::<Posts>()
            .overlaps(Posts::status, "x".to_string())
            .build_path();
        assert!(p.contains("status=ov."), "p={p}");
    }

    // ---- ordering / pagination ----

    #[test]
    fn order_typed_column_ascending() {
        let p = client()
            .from_row::<Posts>()
            .order(Posts::view_count, true)
            .build_path();
        assert!(p.contains("order=view_count.asc"), "p={p}");
    }

    #[test]
    fn order_typed_column_descending() {
        let p = client()
            .from_row::<Posts>()
            .order(Posts::view_count, false)
            .build_path();
        assert!(p.contains("order=view_count.desc"), "p={p}");
    }

    #[test]
    fn order_accepts_str_column_name() {
        // Useful for non-codegen-emitted columns (e.g. views).
        let p = client()
            .from_row::<Posts>()
            .order("custom_col", true)
            .build_path();
        assert!(p.contains("order=custom_col.asc"), "p={p}");
    }

    #[test]
    fn order_with_full_options() {
        let p = client()
            .from_row::<Posts>()
            .order_with(Posts::view_count, Order::desc().nulls_first(true))
            .build_path();
        assert!(p.contains("order=view_count.desc.nullsfirst"), "p={p}");
    }

    #[test]
    fn limit_and_offset() {
        let p = client()
            .from_row::<Posts>()
            .limit(10)
            .offset(20)
            .build_path();
        assert!(p.contains("limit=10"), "p={p}");
        assert!(p.contains("offset=20"), "p={p}");
    }

    #[test]
    fn range_becomes_offset_plus_limit() {
        let p = client()
            .from_row::<Posts>()
            .range(0, 9)
            .build_path();
        assert!(p.contains("limit=10"), "p={p}");
    }

    // ---- count + text search ----

    #[test]
    fn count_attaches_prefer_header() {
        // We can't easily inspect headers from outside, so just prove the call
        // chain doesn't panic and the path stays well-formed.
        let p = client()
            .from_row::<Posts>()
            .count(CountMode::Exact)
            .build_path();
        assert!(p.starts_with("/rest/v1/posts?"), "p={p}");
    }

    #[test]
    fn text_search_emits_fts_op() {
        let p = client()
            .from_row::<Posts>()
            .text_search(Posts::status, "rust|sdk", TextSearchType::Plain, None)
            .build_path();
        assert!(p.contains("status=plfts."), "p={p}");
    }

    // ---- conversion / escape hatch ----

    #[test]
    fn into_untyped_drops_to_postgrest_builder() {
        let untyped: PostgrestBuilder<Value> =
            client().from_row::<Posts>().eq(Posts::id, "x".to_string()).into_untyped();
        let p = untyped.build_path();
        assert!(p.contains("id=eq.x"), "p={p}");
    }

    // ---- type-parameter sanity (compile-time, not assertion) ----

    fn _accepting_only_posts_columns(c: SupabaseClient) {
        let _ = c
            .from_row::<Posts>()
            .eq(Posts::id, "x".to_string())
            .eq(Posts::status, "y".to_string())
            .gt(Posts::view_count, 0i32)
            .is_null(Posts::archived)
            .is_not_null(Posts::published_at);
        // The compile-fail cases (wrong column type, wrong table, is_null on
        // non-nullable column) live under tests/trybuild/.
    }
}

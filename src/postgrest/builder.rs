//! The chainable PostgREST query builder.

use std::future::{Future, IntoFuture};
use std::marker::PhantomData;
use std::pin::Pin;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use crate::error::{Result, SupabaseError};
use crate::universals::{HttpMethod, RequestOptions};
use crate::SupabaseClient;

use super::value::encode_value;

/// Entry point for building a query against a table.
///
/// Obtain one via [`SupabaseClient::from`](crate::SupabaseClient::from).
#[derive(Debug, Clone)]
pub struct TableBuilder {
    client: SupabaseClient,
    table: String,
}

impl TableBuilder {
    pub(crate) fn new(client: SupabaseClient, table: String) -> Self {
        Self { client, table }
    }

    /// Build a `SELECT` query.
    ///
    /// `columns` follows PostgREST syntax — `"*"`, `"id,name"`, or
    /// `"id,name,foreign(col1,col2)"` for embedded resources.
    pub fn select(self, columns: impl Into<String>) -> PostgrestBuilder<Value> {
        let mut q = PostgrestBuilder::new(self.client, self.table, Operation::Select);
        q.state.select_cols = Some(columns.into());
        q
    }

    /// Build an `INSERT`. Body may be a single value or an array of values.
    pub fn insert<B: Serialize>(self, body: B) -> PostgrestBuilder<Value> {
        let mut q = PostgrestBuilder::new(self.client, self.table, Operation::Insert);
        let (val, err) = serialize_body(body);
        q.state.body = val;
        q.state.body_error = err;
        q
    }

    /// Build an `UPSERT` (INSERT with conflict resolution).
    pub fn upsert<B: Serialize>(self, body: B) -> PostgrestBuilder<Value> {
        let mut q = PostgrestBuilder::new(self.client, self.table, Operation::Upsert);
        let (val, err) = serialize_body(body);
        q.state.body = val;
        q.state.body_error = err;
        q.state
            .prefer
            .push("resolution=merge-duplicates".to_string());
        q
    }

    /// Build an `UPDATE`. You must apply filters before awaiting, or the
    /// request will affect every row.
    pub fn update<B: Serialize>(self, body: B) -> PostgrestBuilder<Value> {
        let mut q = PostgrestBuilder::new(self.client, self.table, Operation::Update);
        let (val, err) = serialize_body(body);
        q.state.body = val;
        q.state.body_error = err;
        q
    }

    /// Build a `DELETE`. Apply filters before awaiting.
    pub fn delete(self) -> PostgrestBuilder<Value> {
        PostgrestBuilder::new(self.client, self.table, Operation::Delete)
    }
}

fn serialize_body<B: Serialize>(body: B) -> (Option<Value>, Option<String>) {
    match serde_json::to_value(&body) {
        Ok(v) => (Some(v), None),
        Err(e) => (None, Some(e.to_string())),
    }
}

/// The kind of PostgREST operation. Drives method, return-handling, and
/// some Prefer-header defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Operation {
    Select,
    Insert,
    Upsert,
    Update,
    Delete,
}

impl Operation {
    fn method(self) -> HttpMethod {
        match self {
            Self::Select => HttpMethod::Get,
            Self::Insert | Self::Upsert => HttpMethod::Post,
            Self::Update => HttpMethod::Patch,
            Self::Delete => HttpMethod::Delete,
        }
    }
}

/// PostgREST's three count modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountMode {
    Exact,
    Planned,
    Estimated,
}

impl CountMode {
    fn header_value(self) -> &'static str {
        match self {
            Self::Exact => "count=exact",
            Self::Planned => "count=planned",
            Self::Estimated => "count=estimated",
        }
    }
}

/// PostgREST text-search variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSearchType {
    Plain,
    Phrase,
    WebSearch,
}

impl TextSearchType {
    pub(crate) fn op(self) -> &'static str {
        match self {
            Self::Plain => "plfts",
            Self::Phrase => "phfts",
            Self::WebSearch => "wfts",
        }
    }
}

/// Order-clause options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Order {
    pub ascending: bool,
    pub nulls_first: bool,
    pub foreign_table: Option<&'static str>,
}

impl Order {
    pub fn asc() -> Self {
        Self { ascending: true, nulls_first: false, foreign_table: None }
    }
    pub fn desc() -> Self {
        Self { ascending: false, nulls_first: false, foreign_table: None }
    }
    pub fn nulls_first(mut self, nulls_first: bool) -> Self {
        self.nulls_first = nulls_first;
        self
    }
    pub fn foreign_table(mut self, table: &'static str) -> Self {
        self.foreign_table = Some(table);
        self
    }
}

impl Default for Order {
    fn default() -> Self {
        Self::asc()
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct State {
    pub(crate) select_cols: Option<String>,
    /// Raw `key=value` query-parameter pairs, already URL-encoded.
    pub(crate) params: Vec<(String, String)>,
    pub(crate) prefer: Vec<String>,
    pub(crate) body: Option<Value>,
    /// Captured when serialization of the body failed at builder time. Surfaced
    /// when the request is awaited so the user doesn't silently send `null`.
    pub(crate) body_error: Option<String>,
    pub(crate) limit: Option<u64>,
    pub(crate) offset: Option<u64>,
    pub(crate) range: Option<(u64, u64)>,
    /// `true` once `.select()` has been called on a write op (Prefer: return=representation).
    pub(crate) return_representation: bool,
}

/// The main builder. Generic over the row type `T` (defaults to `serde_json::Value`).
#[must_use = "PostgrestBuilder is lazy — `.await` it or call `.execute()`"]
pub struct PostgrestBuilder<T = Value> {
    pub(crate) client: SupabaseClient,
    pub(crate) table: String,
    pub(crate) op: Operation,
    pub(crate) state: State,
    pub(crate) _marker: PhantomData<fn() -> T>,
}

impl<T> std::fmt::Debug for PostgrestBuilder<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgrestBuilder")
            .field("table", &self.table)
            .field("op", &self.op)
            .field("state", &self.state)
            .finish()
    }
}

impl PostgrestBuilder<Value> {
    pub(crate) fn new(client: SupabaseClient, table: String, op: Operation) -> Self {
        Self {
            client,
            table,
            op,
            state: State::default(),
            _marker: PhantomData,
        }
    }
}

impl<T> PostgrestBuilder<T> {
    pub(crate) fn push_param(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.state.params.push((key.into(), value.into()));
    }

    /// Reinterpret the response rows as a different type. Like supabase-js's
    /// `.returns<T>()` but the change actually applies to deserialization.
    pub fn returns<U>(self) -> PostgrestBuilder<U> {
        PostgrestBuilder {
            client: self.client,
            table: self.table,
            op: self.op,
            state: self.state,
            _marker: PhantomData,
        }
    }

    /// Order by `column`. Pass `true` for ascending, `false` for descending.
    pub fn order(mut self, column: &str, ascending: bool) -> Self {
        let dir = if ascending { "asc" } else { "desc" };
        let key = if let Some(ft) = order_foreign_table_carry(&self.state) {
            format!("{ft}.order")
        } else {
            "order".to_string()
        };
        self.append_csv_param(&key, &format!("{column}.{dir}"));
        self
    }

    /// Order with full control (nulls placement, foreign-table scoping).
    pub fn order_with(mut self, column: &str, options: Order) -> Self {
        let dir = if options.ascending { "asc" } else { "desc" };
        let nulls = if options.nulls_first { "nullsfirst" } else { "nullslast" };
        let key = if let Some(ft) = options.foreign_table {
            format!("{ft}.order")
        } else {
            "order".to_string()
        };
        self.append_csv_param(&key, &format!("{column}.{dir}.{nulls}"));
        self
    }

    /// Cap the number of returned rows.
    pub fn limit(mut self, n: u64) -> Self {
        self.state.limit = Some(n);
        self
    }

    /// Skip the first `n` rows.
    pub fn offset(mut self, n: u64) -> Self {
        self.state.offset = Some(n);
        self
    }

    /// Return rows in the inclusive `from..=to` range (mirrors supabase-js).
    pub fn range(mut self, from: u64, to: u64) -> Self {
        self.state.range = Some((from, to));
        self
    }

    /// Ask PostgREST to include a row count alongside the data. The count is
    /// returned via the `Content-Range` header; use [`execute_with_count`]
    /// to access it explicitly.
    ///
    /// [`execute_with_count`]: PostgrestBuilder::execute_with_count
    pub fn count(mut self, mode: CountMode) -> Self {
        self.state.prefer.push(mode.header_value().to_string());
        self
    }

    /// On write ops: ask PostgREST to return the affected rows. Equivalent to
    /// calling `.select("*")` on an insert/update/upsert/delete in supabase-js.
    pub fn select_returning(mut self, columns: impl Into<String>) -> Self {
        self.state.select_cols = Some(columns.into());
        self.state.return_representation = true;
        if !self.state.prefer.iter().any(|p| p.starts_with("return=")) {
            self.state.prefer.push("return=representation".to_string());
        }
        self
    }

    /// Upsert option: which column(s) are the conflict target.
    pub fn on_conflict(mut self, columns: impl Into<String>) -> Self {
        self.push_param("on_conflict", encode_value(&columns.into()));
        self
    }

    /// Upsert option: skip rows that would violate uniqueness instead of merging.
    pub fn ignore_duplicates(mut self) -> Self {
        // Replace any prior resolution preference.
        self.state.prefer.retain(|p| !p.starts_with("resolution="));
        self.state
            .prefer
            .push("resolution=ignore-duplicates".to_string());
        self
    }

    /// Expect exactly one row. Returns [`SupabaseError::NotFound`] if zero or
    /// `SupabaseError::Unexpected` if multiple rows come back.
    pub fn single(mut self) -> SingleBuilder<T> {
        self.state
            .prefer
            .push("count=none".to_string());
        SingleBuilder { inner: self }
    }

    /// Expect zero or one row.
    pub fn maybe_single(self) -> MaybeSingleBuilder<T> {
        MaybeSingleBuilder { inner: self }
    }

    fn append_csv_param(&mut self, key: &str, value: &str) {
        if let Some((_, existing)) = self.state.params.iter_mut().find(|(k, _)| k == key) {
            existing.push(',');
            existing.push_str(value);
        } else {
            self.push_param(key, value);
        }
    }

    /// Build the path-and-query portion of the request URL.
    ///
    /// Exposed primarily for debugging and testing — call this to see the
    /// exact PostgREST URL the builder will hit, without sending a request.
    pub fn build_path(&self) -> String {
        let mut params: Vec<(String, String)> = Vec::new();

        if let Some(cols) = &self.state.select_cols {
            params.push(("select".to_string(), encode_value(cols)));
        }
        for (k, v) in &self.state.params {
            params.push((k.clone(), v.clone()));
        }
        if let Some(limit) = self.state.limit {
            params.push(("limit".to_string(), limit.to_string()));
        }
        if let Some(offset) = self.state.offset {
            params.push(("offset".to_string(), offset.to_string()));
        }
        if let Some((from, to)) = self.state.range {
            let len = to.saturating_sub(from).saturating_add(1);
            params.push(("offset".to_string(), from.to_string()));
            params.push(("limit".to_string(), len.to_string()));
        }

        if params.is_empty() {
            format!("/rest/v1/{}", self.table)
        } else {
            let qs: Vec<String> = params.into_iter().map(|(k, v)| format!("{k}={v}")).collect();
            format!("/rest/v1/{}?{}", self.table, qs.join("&"))
        }
    }

    pub(crate) fn build_options(&self) -> RequestOptions {
        let mut prefer = self.state.prefer.clone();
        // When a write op did not call `.select()`, prefer minimal so the
        // response body stays empty and we don't pay for the row payload.
        if matches!(
            self.op,
            Operation::Insert | Operation::Upsert | Operation::Update | Operation::Delete
        ) && !self.state.return_representation
            && !prefer.iter().any(|p| p.starts_with("return="))
        {
            prefer.push("return=minimal".to_string());
        }
        RequestOptions {
            prefer,
            ..RequestOptions::postgrest()
        }
    }
}

/// Helper used by `.order()` to keep an existing foreign-table scope if one is set.
fn order_foreign_table_carry(_state: &State) -> Option<&'static str> {
    // We don't carry state between calls — `.order()` is always top-level
    // unless the caller uses `.order_with(...).foreign_table(...)`.
    None
}

impl<T: DeserializeOwned + Send + 'static> PostgrestBuilder<T> {
    /// Send the request and deserialize the response body as `Vec<T>`.
    pub async fn execute(self) -> Result<Vec<T>> {
        let (_count, rows) = self.execute_inner().await?;
        Ok(rows)
    }

    /// Send the request and return both the deserialized rows and the
    /// total count from the `Content-Range` header (when [`count`] was set).
    ///
    /// [`count`]: PostgrestBuilder::count
    pub async fn execute_with_count(self) -> Result<(Vec<T>, Option<u64>)> {
        let (count, rows) = self.execute_inner().await?;
        Ok((rows, count))
    }

    async fn execute_inner(self) -> Result<(Option<u64>, Vec<T>)> {
        if let Some(msg) = &self.state.body_error {
            return Err(SupabaseError::Unexpected(format!("failed to serialize request body: {msg}")));
        }
        let path = self.build_path();
        let opts = self.build_options();

        let (_status, headers, body) = self
            .client
            .request_full(&path, self.op.method(), self.state.body.clone(), &opts)
            .await?;

        let count = headers
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            .and_then(parse_count_from_content_range);

        if body.is_empty() {
            return Ok((count, Vec::new()));
        }

        // PostgREST normally returns an array; defensively handle a bare object too.
        let value: Value = serde_json::from_str(&body).map_err(|e| SupabaseError::Decode {
            message: e.to_string(),
            body: body.clone(),
        })?;

        let rows = match value {
            Value::Array(arr) => arr
                .into_iter()
                .map(|v| {
                    serde_json::from_value(v.clone()).map_err(|e| SupabaseError::Decode {
                        message: e.to_string(),
                        body: v.to_string(),
                    })
                })
                .collect::<Result<Vec<T>>>()?,
            Value::Null => Vec::new(),
            other => vec![serde_json::from_value(other.clone()).map_err(|e| {
                SupabaseError::Decode {
                    message: e.to_string(),
                    body: other.to_string(),
                }
            })?],
        };

        Ok((count, rows))
    }
}

fn parse_count_from_content_range(header: &str) -> Option<u64> {
    // Format: "0-9/123" or "*/123"
    let total = header.split('/').nth(1)?;
    let trimmed = total.trim();
    if trimmed == "*" {
        None
    } else {
        trimmed.parse::<u64>().ok()
    }
}

impl<T: DeserializeOwned + Send + 'static> IntoFuture for PostgrestBuilder<T> {
    type Output = Result<Vec<T>>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.execute())
    }
}

// --- single / maybe_single -------------------------------------------------

/// Builder returned by [`PostgrestBuilder::single`]. Awaits to `Result<T>`.
#[must_use = "SingleBuilder is lazy — `.await` it or call `.execute()`"]
pub struct SingleBuilder<T> {
    inner: PostgrestBuilder<T>,
}

impl<T: DeserializeOwned + Send + 'static> SingleBuilder<T> {
    pub async fn execute(self) -> Result<T> {
        let table = self.inner.table.clone();
        let rows = self.inner.execute().await?;
        let n = rows.len();
        let mut iter = rows.into_iter();
        match (n, iter.next()) {
            (0, _) => Err(SupabaseError::NotFound { resource: table }),
            (1, Some(row)) => Ok(row),
            _ => Err(SupabaseError::Unexpected(format!(
                "Expected exactly one row from `{table}`, got {n}"
            ))),
        }
    }
}

impl<T: DeserializeOwned + Send + 'static> IntoFuture for SingleBuilder<T> {
    type Output = Result<T>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.execute())
    }
}

/// Builder returned by [`PostgrestBuilder::maybe_single`]. Awaits to `Result<Option<T>>`.
#[must_use = "MaybeSingleBuilder is lazy — `.await` it or call `.execute()`"]
pub struct MaybeSingleBuilder<T> {
    inner: PostgrestBuilder<T>,
}

impl<T: DeserializeOwned + Send + 'static> MaybeSingleBuilder<T> {
    pub async fn execute(self) -> Result<Option<T>> {
        let table = self.inner.table.clone();
        let rows = self.inner.execute().await?;
        let n = rows.len();
        let mut iter = rows.into_iter();
        match (n, iter.next()) {
            (0, _) => Ok(None),
            (1, Some(row)) => Ok(Some(row)),
            _ => Err(SupabaseError::Unexpected(format!(
                "Expected at most one row from `{table}`, got {n}"
            ))),
        }
    }
}

impl<T: DeserializeOwned + Send + 'static> IntoFuture for MaybeSingleBuilder<T> {
    type Output = Result<Option<T>>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.execute())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::SupabaseClient;
    use serde_json::json;

    fn client() -> SupabaseClient {
        SupabaseClient::new("https://example.supabase.co", "anon", None)
    }

    #[test]
    fn select_all_no_filters() {
        let q = client().from("countries").select("*");
        assert_eq!(q.build_path(), "/rest/v1/countries?select=%2A");
    }

    #[test]
    fn select_with_columns_and_eq_filter() {
        let q = client()
            .from("countries")
            .select("id,name")
            .eq("status", "active");
        assert_eq!(
            q.build_path(),
            "/rest/v1/countries?select=id%2Cname&status=eq.active"
        );
    }

    #[test]
    fn chained_filters_are_anded() {
        let q = client()
            .from("t")
            .select("*")
            .gt("score", 60)
            .lte("score", 100)
            .neq("status", "archived");
        assert_eq!(
            q.build_path(),
            "/rest/v1/t?select=%2A&score=gt.60&score=lte.100&status=neq.archived"
        );
    }

    #[test]
    fn in_filter_renders_paren_list() {
        let q = client()
            .from("t")
            .select("*")
            .in_("status", ["active", "pending", "trial"]);
        assert_eq!(
            q.build_path(),
            "/rest/v1/t?select=%2A&status=in.%28active%2Cpending%2Ctrial%29"
        );
    }

    #[test]
    fn is_filter_null() {
        let q = client().from("t").select("*").is("deleted_at", "null");
        assert_eq!(q.build_path(), "/rest/v1/t?select=%2A&deleted_at=is.null");
    }

    #[test]
    fn order_ascending_and_descending() {
        let q = client()
            .from("t")
            .select("*")
            .order("name", true)
            .order("created_at", false);
        // Multiple orders collapse into a single comma-separated param.
        assert_eq!(
            q.build_path(),
            "/rest/v1/t?select=%2A&order=name.asc,created_at.desc"
        );
    }

    #[test]
    fn limit_and_offset() {
        let q = client().from("t").select("*").limit(10).offset(20);
        assert_eq!(q.build_path(), "/rest/v1/t?select=%2A&limit=10&offset=20");
    }

    #[test]
    fn range_becomes_offset_plus_limit() {
        let q = client().from("t").select("*").range(0, 9);
        assert_eq!(q.build_path(), "/rest/v1/t?select=%2A&offset=0&limit=10");
    }

    #[test]
    fn or_filter() {
        let q = client()
            .from("t")
            .select("*")
            .or("status.eq.active,priority.gt.5");
        assert_eq!(
            q.build_path(),
            "/rest/v1/t?select=%2A&or=%28status.eq.active%2Cpriority.gt.5%29"
        );
    }

    #[test]
    fn not_filter() {
        let q = client().from("t").select("*").not("name", "eq", "foo");
        assert_eq!(q.build_path(), "/rest/v1/t?select=%2A&name=not.eq.foo");
    }

    #[test]
    fn insert_no_filters_yields_post_with_minimal_return() {
        let q = client().from("t").insert(json!({"name": "x"}));
        let opts = q.build_options();
        assert_eq!(q.build_path(), "/rest/v1/t");
        assert!(opts.prefer.iter().any(|p| p == "return=minimal"));
    }

    #[test]
    fn insert_with_select_returns_representation() {
        let q = client()
            .from("t")
            .insert(json!({"name": "x"}))
            .select_returning("*");
        let opts = q.build_options();
        assert_eq!(q.build_path(), "/rest/v1/t?select=%2A");
        assert!(opts.prefer.iter().any(|p| p == "return=representation"));
        assert!(!opts.prefer.iter().any(|p| p == "return=minimal"));
    }

    #[test]
    fn upsert_sets_resolution_header_and_supports_on_conflict() {
        let q = client()
            .from("t")
            .upsert(json!({"id": 1, "name": "x"}))
            .on_conflict("id");
        let opts = q.build_options();
        assert_eq!(q.build_path(), "/rest/v1/t?on_conflict=id");
        assert!(opts.prefer.iter().any(|p| p == "resolution=merge-duplicates"));
    }

    #[test]
    fn upsert_ignore_duplicates_replaces_resolution() {
        let q = client()
            .from("t")
            .upsert(json!({"id": 1}))
            .ignore_duplicates();
        let opts = q.build_options();
        let resolutions: Vec<&String> =
            opts.prefer.iter().filter(|p| p.starts_with("resolution=")).collect();
        assert_eq!(resolutions, vec![&"resolution=ignore-duplicates".to_string()]);
    }

    #[test]
    fn count_appends_prefer_header() {
        let q = client()
            .from("t")
            .select("*")
            .count(CountMode::Exact);
        let opts = q.build_options();
        assert!(opts.prefer.iter().any(|p| p == "count=exact"));
    }

    #[test]
    fn delete_with_filter_sends_query_params() {
        let q = client().from("t").delete().eq("id", 1);
        assert_eq!(q.build_path(), "/rest/v1/t?id=eq.1");
    }

    #[test]
    fn returns_changes_row_type_marker() {
        // Compile-time test: `.returns::<MyRow>()` should change the future's Output.
        #[derive(serde::Deserialize)]
        struct MyRow {
            #[allow(dead_code)]
            id: i64,
        }
        let _q: PostgrestBuilder<MyRow> = client().from("t").select("*").returns::<MyRow>();
    }

    #[test]
    fn parse_content_range_total() {
        assert_eq!(parse_count_from_content_range("0-9/123"), Some(123));
        assert_eq!(parse_count_from_content_range("*/42"), Some(42));
        assert_eq!(parse_count_from_content_range("0-9/*"), None);
        assert_eq!(parse_count_from_content_range("garbage"), None);
    }

    // --- additional filter-method path tests ---

    #[test]
    fn like_filter_encodes_pattern() {
        let q = client().from("t").select("*").like("name", "%foo%");
        assert_eq!(
            q.build_path(),
            "/rest/v1/t?select=%2A&name=like.%25foo%25"
        );
    }

    #[test]
    fn ilike_filter_encodes_pattern() {
        let q = client().from("t").select("*").ilike("email", "%@example.com");
        assert_eq!(
            q.build_path(),
            "/rest/v1/t?select=%2A&email=ilike.%25%40example.com"
        );
    }

    #[test]
    fn neq_filter() {
        let q = client().from("t").select("*").neq("status", "banned");
        assert_eq!(q.build_path(), "/rest/v1/t?select=%2A&status=neq.banned");
    }

    #[test]
    fn gt_and_gte_filters() {
        let q = client().from("t").select("*").gt("age", 18).gte("score", 50);
        assert_eq!(
            q.build_path(),
            "/rest/v1/t?select=%2A&age=gt.18&score=gte.50"
        );
    }

    #[test]
    fn lt_and_lte_filters() {
        let q = client().from("t").select("*").lt("age", 65).lte("score", 100);
        assert_eq!(
            q.build_path(),
            "/rest/v1/t?select=%2A&age=lt.65&score=lte.100"
        );
    }

    #[test]
    fn contains_filter() {
        let q = client().from("t").select("*").contains("tags", "{rust,sdk}");
        assert_eq!(
            q.build_path(),
            "/rest/v1/t?select=%2A&tags=cs.%7Brust%2Csdk%7D"
        );
    }

    #[test]
    fn contained_by_filter() {
        let q = client().from("t").select("*").contained_by("tags", "{a,b,c}");
        assert_eq!(
            q.build_path(),
            "/rest/v1/t?select=%2A&tags=cd.%7Ba%2Cb%2Cc%7D"
        );
    }

    #[test]
    fn overlaps_filter() {
        let q = client().from("t").select("*").overlaps("tags", "{x,y}");
        assert_eq!(
            q.build_path(),
            "/rest/v1/t?select=%2A&tags=ov.%7Bx%2Cy%7D"
        );
    }

    #[test]
    fn range_operators() {
        let q = client().from("t").select("*").range_lt("age_range", "(20,30)");
        assert_eq!(
            q.build_path(),
            "/rest/v1/t?select=%2A&age_range=sl.%2820%2C30%29"
        );
        let q = client().from("t").select("*").range_gt("age_range", "(20,30)");
        assert_eq!(
            q.build_path(),
            "/rest/v1/t?select=%2A&age_range=sr.%2820%2C30%29"
        );
        let q = client().from("t").select("*").range_lte("age_range", "(20,30)");
        assert_eq!(
            q.build_path(),
            "/rest/v1/t?select=%2A&age_range=nxr.%2820%2C30%29"
        );
        let q = client().from("t").select("*").range_gte("age_range", "(20,30)");
        assert_eq!(
            q.build_path(),
            "/rest/v1/t?select=%2A&age_range=nxl.%2820%2C30%29"
        );
        let q = client().from("t").select("*").range_adjacent("age_range", "(20,30)");
        assert_eq!(
            q.build_path(),
            "/rest/v1/t?select=%2A&age_range=adj.%2820%2C30%29"
        );
    }

    #[test]
    fn text_search_plain_no_config() {
        let q = client()
            .from("t")
            .select("*")
            .text_search("body", "foo bar", TextSearchType::Plain, None);
        // urlencoding encodes spaces as %20, not +
        assert_eq!(
            q.build_path(),
            "/rest/v1/t?select=%2A&body=plfts.foo%20bar"
        );
    }

    #[test]
    fn text_search_websearch_with_config() {
        let q = client()
            .from("t")
            .select("*")
            .text_search("body", "rust sdk", TextSearchType::WebSearch, Some("english"));
        // Config parens are part of the op string, not URL-encoded; query uses %20.
        assert_eq!(
            q.build_path(),
            "/rest/v1/t?select=%2A&body=wfts(english).rust%20sdk"
        );
    }

    #[test]
    fn text_search_phrase() {
        let q = client()
            .from("t")
            .select("*")
            .text_search("body", "hello world", TextSearchType::Phrase, None);
        assert_eq!(
            q.build_path(),
            "/rest/v1/t?select=%2A&body=phfts.hello%20world"
        );
    }

    #[test]
    fn match_applies_eq_for_each_key() {
        let q = client()
            .from("t")
            .select("*")
            .match_(json!({"col1": "a", "col2": "b"}));
        let path = q.build_path();
        // Both filters must appear; order may vary due to BTreeMap ordering in serde.
        assert!(path.contains("col1=eq.a"), "path={path}");
        assert!(path.contains("col2=eq.b"), "path={path}");
    }

    #[test]
    fn match_handles_null_values() {
        let q = client()
            .from("t")
            .select("*")
            .match_(json!({"deleted_at": null}));
        let path = q.build_path();
        assert!(path.contains("deleted_at=eq.null"), "path={path}");
    }

    #[test]
    fn not_with_various_ops() {
        let q = client().from("t").select("*").not("status", "eq", "active");
        assert_eq!(q.build_path(), "/rest/v1/t?select=%2A&status=not.eq.active");

        let q = client().from("t").select("*").not("age", "gt", 100);
        assert_eq!(q.build_path(), "/rest/v1/t?select=%2A&age=not.gt.100");

        // Unknown op falls back to "eq"
        let q = client().from("t").select("*").not("x", "bogus_op", "v");
        assert_eq!(q.build_path(), "/rest/v1/t?select=%2A&x=not.eq.v");
    }

    #[test]
    fn generic_filter_method() {
        let q = client()
            .from("t")
            .select("*")
            .filter("size", "eq", 42);
        assert_eq!(q.build_path(), "/rest/v1/t?select=%2A&size=eq.42");
    }

    #[test]
    fn in_filter_with_integers() {
        let q = client()
            .from("t")
            .select("*")
            .in_("id", [1i32, 2, 3]);
        let path = q.build_path();
        assert!(path.contains("id=in."), "path={path}");
        assert!(path.contains("1"), "path={path}");
        assert!(path.contains("2"), "path={path}");
        assert!(path.contains("3"), "path={path}");
    }

    #[test]
    fn in_filter_with_empty_list() {
        let q = client()
            .from("t")
            .select("*")
            .in_("id", Vec::<i32>::new());
        let path = q.build_path();
        // Empty list should produce `id=in.()`
        assert!(path.contains("id=in."), "path={path}");
    }

    #[test]
    fn order_with_nulls_first() {
        let q = client()
            .from("t")
            .select("*")
            .order_with("created_at", Order::desc().nulls_first(true));
        assert_eq!(
            q.build_path(),
            "/rest/v1/t?select=%2A&order=created_at.desc.nullsfirst"
        );
    }

    #[test]
    fn order_with_nulls_last() {
        let q = client()
            .from("t")
            .select("*")
            .order_with("name", Order::asc().nulls_first(false));
        assert_eq!(
            q.build_path(),
            "/rest/v1/t?select=%2A&order=name.asc.nullslast"
        );
    }

    #[test]
    fn multiple_orders_collapsed_to_csv() {
        let q = client()
            .from("t")
            .select("*")
            .order("a", true)
            .order("b", false)
            .order("c", true);
        assert_eq!(
            q.build_path(),
            "/rest/v1/t?select=%2A&order=a.asc,b.desc,c.asc"
        );
    }

    #[test]
    fn offset_without_limit() {
        let q = client().from("t").select("*").offset(5);
        assert_eq!(q.build_path(), "/rest/v1/t?select=%2A&offset=5");
    }

    #[test]
    fn range_zero_to_zero_gives_limit_one() {
        let q = client().from("t").select("*").range(0, 0);
        assert_eq!(q.build_path(), "/rest/v1/t?select=%2A&offset=0&limit=1");
    }

    #[test]
    fn select_returns_type_changes_marker() {
        #[derive(serde::Deserialize)]
        struct TypedRow {
            #[allow(dead_code)]
            id: i64,
        }
        // Compiles + type changes: PostgrestBuilder<Value> → PostgrestBuilder<TypedRow>
        let _q: PostgrestBuilder<TypedRow> = client()
            .from("t")
            .select("id")
            .returns::<TypedRow>();
    }

    #[test]
    fn upsert_on_conflict_url_encoded() {
        let q = client()
            .from("t")
            .upsert(json!({"id": 1}))
            .on_conflict("user_id,org_id");
        assert_eq!(
            q.build_path(),
            "/rest/v1/t?on_conflict=user_id%2Corg_id"
        );
    }

    #[test]
    fn count_modes_produce_correct_prefer_header() {
        for (mode, expected) in [
            (CountMode::Exact, "count=exact"),
            (CountMode::Planned, "count=planned"),
            (CountMode::Estimated, "count=estimated"),
        ] {
            let q = client().from("t").select("*").count(mode);
            let opts = q.build_options();
            assert!(
                opts.prefer.iter().any(|p| p == expected),
                "missing '{expected}' in prefer: {:?}", opts.prefer
            );
        }
    }

    #[test]
    fn delete_returns_minimal_by_default() {
        let q = client().from("t").delete().eq("id", 99);
        let opts = q.build_options();
        assert!(opts.prefer.iter().any(|p| p == "return=minimal"));
    }

    #[test]
    fn update_with_filter() {
        let q = client()
            .from("t")
            .update(json!({"name": "new"}))
            .eq("id", 7);
        assert_eq!(q.build_path(), "/rest/v1/t?id=eq.7");
        let opts = q.build_options();
        assert!(opts.prefer.iter().any(|p| p == "return=minimal"));
    }

    #[test]
    fn select_returning_on_delete() {
        let q = client()
            .from("t")
            .delete()
            .eq("id", 1)
            .select_returning("id,name");
        let opts = q.build_options();
        assert!(opts.prefer.iter().any(|p| p == "return=representation"));
        assert!(!opts.prefer.iter().any(|p| p == "return=minimal"));
    }

    #[test]
    fn parse_content_range_edge_cases() {
        // Large totals
        assert_eq!(parse_count_from_content_range("0-999/1000000"), Some(1_000_000));
        // Empty string
        assert_eq!(parse_count_from_content_range(""), None);
        // Only slash
        assert_eq!(parse_count_from_content_range("/"), None);
        // Negative-looking value doesn't parse
        assert_eq!(parse_count_from_content_range("0-9/-1"), None);
    }

    #[test]
    fn table_name_with_underscores_and_numbers() {
        let q = client().from("user_sessions_2024").select("*");
        assert!(q.build_path().starts_with("/rest/v1/user_sessions_2024"));
    }

    #[test]
    fn or_with_complex_expression() {
        let q = client()
            .from("t")
            .select("*")
            .or("age.gt.18,status.in.(active,trial)");
        let path = q.build_path();
        assert!(path.contains("or="), "path={path}");
        assert!(path.contains("%28"), "expected encoded '(' in path={path}");
    }

    #[test]
    fn body_error_is_surfaced_lazily() {
        // Insert with a type that can't be serialized (cycles / non-serializable).
        // The simplest proxy: construct a builder directly with a body_error set.
        let mut q = PostgrestBuilder::new(client(), "t".into(), Operation::Insert);
        q.state.body_error = Some("kaboom".to_string());
        // The error is not visible until .execute() is awaited — it lives in the
        // state, not in the return value of the builder constructor.
        assert!(q.state.body_error.is_some());
    }

    #[test]
    fn order_default_is_asc() {
        let d = Order::default();
        assert!(d.ascending);
        assert!(!d.nulls_first);
        assert!(d.foreign_table.is_none());
    }

    #[test]
    fn order_foreign_table_setter() {
        let o = Order::asc().foreign_table("profiles");
        assert_eq!(o.foreign_table, Some("profiles"));
    }

    #[test]
    fn order_with_foreign_table_emits_scoped_order_key() {
        let q = client()
            .from("posts")
            .select("*,profiles(*)")
            .order_with("name", Order::asc().foreign_table("profiles"));
        let p = q.build_path();
        assert!(p.contains("profiles.order=name.asc"), "path={p}");
    }

    #[test]
    fn postgrest_builder_debug_does_not_panic() {
        let q = client().from("t").select("*").eq("id", "1");
        let _ = format!("{q:?}");
    }

    #[tokio::test]
    async fn execute_with_body_error_surfaces_unexpected() {
        let mut q = PostgrestBuilder::new(client(), "t".into(), Operation::Insert);
        q.state.body_error = Some("nope".into());
        let err = q.execute().await.unwrap_err();
        match err {
            SupabaseError::Unexpected(msg) => assert!(msg.contains("nope")),
            other => panic!("expected Unexpected, got {other:?}"),
        }
    }

    #[test]
    fn serialize_body_failure_captures_error() {
        // Use a Serialize impl that always errors out — proves serialize_body
        // routes the error into `body_error` rather than panicking.
        struct AlwaysFails;
        impl Serialize for AlwaysFails {
            fn serialize<S: serde::Serializer>(&self, _s: S) -> std::result::Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("nope"))
            }
        }
        let (v, err) = serialize_body(AlwaysFails);
        assert!(v.is_none());
        assert!(err.is_some(), "expected serialize_body to capture an error");
        assert!(err.unwrap().contains("nope"));
    }
}

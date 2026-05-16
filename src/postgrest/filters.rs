//! Filter methods on [`PostgrestBuilder`].
//!
//! Each method appends a single query-string filter and returns `self` so
//! they can be chained. Filters are AND-combined unless wrapped in `.or(...)`.

use serde_json::Value;

use super::builder::{PostgrestBuilder, TextSearchType};
use super::value::{encode_column, encode_value, render_list, PostgrestValue};

/// The set of filter operators a `not.<op>` / `filter(col, op, val)` accepts.
const SCALAR_OPS: &[&str] = &[
    "eq", "neq", "gt", "gte", "lt", "lte", "like", "ilike", "match", "imatch", "in", "is", "isdistinct",
    "fts", "plfts", "phfts", "wfts", "cs", "cd", "ov", "sl", "sr", "nxr", "nxl", "adj",
];

impl<T> PostgrestBuilder<T> {
    fn add_filter(mut self, column: &str, op: &str, value: &str) -> Self {
        self.push_param(encode_column(column), format!("{op}.{value}"));
        self
    }

    /// `column = value`
    pub fn eq<V: PostgrestValue>(self, column: &str, value: V) -> Self {
        let encoded = encode_value(&value.render());
        self.add_filter(column, "eq", &encoded)
    }

    /// `column <> value`
    pub fn neq<V: PostgrestValue>(self, column: &str, value: V) -> Self {
        let encoded = encode_value(&value.render());
        self.add_filter(column, "neq", &encoded)
    }

    /// `column > value`
    pub fn gt<V: PostgrestValue>(self, column: &str, value: V) -> Self {
        let encoded = encode_value(&value.render());
        self.add_filter(column, "gt", &encoded)
    }

    /// `column >= value`
    pub fn gte<V: PostgrestValue>(self, column: &str, value: V) -> Self {
        let encoded = encode_value(&value.render());
        self.add_filter(column, "gte", &encoded)
    }

    /// `column < value`
    pub fn lt<V: PostgrestValue>(self, column: &str, value: V) -> Self {
        let encoded = encode_value(&value.render());
        self.add_filter(column, "lt", &encoded)
    }

    /// `column <= value`
    pub fn lte<V: PostgrestValue>(self, column: &str, value: V) -> Self {
        let encoded = encode_value(&value.render());
        self.add_filter(column, "lte", &encoded)
    }

    /// `column LIKE pattern` (case-sensitive). `*` is the wildcard in PostgREST syntax.
    pub fn like<V: PostgrestValue>(self, column: &str, pattern: V) -> Self {
        let encoded = encode_value(&pattern.render());
        self.add_filter(column, "like", &encoded)
    }

    /// `column ILIKE pattern` (case-insensitive).
    pub fn ilike<V: PostgrestValue>(self, column: &str, pattern: V) -> Self {
        let encoded = encode_value(&pattern.render());
        self.add_filter(column, "ilike", &encoded)
    }

    /// `column IS value`. `value` is one of `"null"`, `"true"`, `"false"`, or `"unknown"`.
    pub fn is(self, column: &str, value: &str) -> Self {
        self.add_filter(column, "is", value)
    }

    /// `column IN (a, b, c)`.
    pub fn in_<I, V>(self, column: &str, values: I) -> Self
    where
        I: IntoIterator<Item = V>,
        V: PostgrestValue,
    {
        let list = render_list(values);
        self.add_filter(column, "in", &encode_value(&list))
    }

    /// `column @> value` (array/range/jsonb contains).
    pub fn contains<V: PostgrestValue>(self, column: &str, value: V) -> Self {
        let encoded = encode_value(&value.render());
        self.add_filter(column, "cs", &encoded)
    }

    /// `column <@ value` (array/range/jsonb contained by).
    pub fn contained_by<V: PostgrestValue>(self, column: &str, value: V) -> Self {
        let encoded = encode_value(&value.render());
        self.add_filter(column, "cd", &encoded)
    }

    /// Range strictly left of (`column << range`).
    pub fn range_lt<V: PostgrestValue>(self, column: &str, range: V) -> Self {
        let encoded = encode_value(&range.render());
        self.add_filter(column, "sl", &encoded)
    }

    /// Range strictly right of (`column >> range`).
    pub fn range_gt<V: PostgrestValue>(self, column: &str, range: V) -> Self {
        let encoded = encode_value(&range.render());
        self.add_filter(column, "sr", &encoded)
    }

    /// Range does not extend to the right of (`column &< range`).
    pub fn range_lte<V: PostgrestValue>(self, column: &str, range: V) -> Self {
        let encoded = encode_value(&range.render());
        self.add_filter(column, "nxr", &encoded)
    }

    /// Range does not extend to the left of (`column &> range`).
    pub fn range_gte<V: PostgrestValue>(self, column: &str, range: V) -> Self {
        let encoded = encode_value(&range.render());
        self.add_filter(column, "nxl", &encoded)
    }

    /// Range is adjacent to (`column -|- range`).
    pub fn range_adjacent<V: PostgrestValue>(self, column: &str, range: V) -> Self {
        let encoded = encode_value(&range.render());
        self.add_filter(column, "adj", &encoded)
    }

    /// `column && value` (array/range overlap).
    pub fn overlaps<V: PostgrestValue>(self, column: &str, value: V) -> Self {
        let encoded = encode_value(&value.render());
        self.add_filter(column, "ov", &encoded)
    }

    /// Full-text search on `column`. Use [`TextSearchType`] to pick `plain`,
    /// `phrase`, or `websearch`; `config` is the Postgres text-search config
    /// (e.g. `Some("english")`).
    pub fn text_search(
        self,
        column: &str,
        query: &str,
        ts_type: TextSearchType,
        config: Option<&str>,
    ) -> Self {
        let mut op = ts_type.op().to_string();
        if let Some(cfg) = config {
            op.push_str(&format!("({cfg})"));
        }
        let encoded = encode_value(query);
        self.add_filter(column, &op, &encoded)
    }

    /// Equality match on every key in the JSON object.
    ///
    /// `client.from("t").select("*").match_(json!({"col1": "a", "col2": "b"}))`
    pub fn match_(mut self, criteria: Value) -> Self {
        if let Value::Object(map) = criteria {
            for (k, v) in map {
                let raw = match v {
                    Value::String(s) => s,
                    Value::Null => "null".to_string(),
                    other => other.to_string(),
                };
                self = self.eq(&k, raw);
            }
        }
        self
    }

    /// Negate any filter: `not.<op>.<value>`.
    pub fn not<V: PostgrestValue>(self, column: &str, op: &str, value: V) -> Self {
        let op = if SCALAR_OPS.contains(&op) { op } else { "eq" };
        let encoded = encode_value(&value.render());
        self.add_filter(column, &format!("not.{op}"), &encoded)
    }

    /// OR-combine raw PostgREST filter syntax.
    ///
    /// ```ignore
    /// .or("status.eq.active,priority.gt.5")
    /// ```
    pub fn or(mut self, filters: &str) -> Self {
        let wrapped = format!("({filters})");
        self.push_param("or", encode_value(&wrapped));
        self
    }

    /// Generic escape hatch: emit `column=op.value` for any operator string.
    pub fn filter<V: PostgrestValue>(self, column: &str, op: &str, value: V) -> Self {
        let encoded = encode_value(&value.render());
        self.add_filter(column, op, &encoded)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::SupabaseClient;
    use crate::postgrest::builder::PostgrestBuilder;

    fn client() -> SupabaseClient {
        SupabaseClient::new("https://example.supabase.co", "anon", None)
    }

    fn path(q: PostgrestBuilder) -> String {
        q.build_path()
    }

    // --- comprehensive filter coverage ---

    #[test]
    fn eq_string_value() {
        assert_eq!(
            path(client().from("t").select("*").eq("status", "active")),
            "/rest/v1/t?select=%2A&status=eq.active"
        );
    }

    #[test]
    fn eq_integer_value() {
        assert_eq!(
            path(client().from("t").select("*").eq("id", 42u64)),
            "/rest/v1/t?select=%2A&id=eq.42"
        );
    }

    #[test]
    fn eq_boolean_value() {
        assert_eq!(
            path(client().from("t").select("*").eq("active", true)),
            "/rest/v1/t?select=%2A&active=eq.true"
        );
    }

    #[test]
    fn eq_value_with_special_chars_is_encoded() {
        // Values containing URL-special chars must be percent-encoded.
        assert_eq!(
            path(client().from("t").select("*").eq("name", "O'Brien")),
            "/rest/v1/t?select=%2A&name=eq.O%27Brien"
        );
    }

    #[test]
    fn neq_filter_produces_neq_op() {
        assert!(
            path(client().from("t").select("*").neq("x", "y"))
                .contains("x=neq.y")
        );
    }

    #[test]
    fn gt_gte_lt_lte_ops() {
        for (method_result, expected_op) in [
            (path(client().from("t").select("*").gt("n", 10)), "n=gt.10"),
            (path(client().from("t").select("*").gte("n", 10)), "n=gte.10"),
            (path(client().from("t").select("*").lt("n", 10)), "n=lt.10"),
            (path(client().from("t").select("*").lte("n", 10)), "n=lte.10"),
        ] {
            assert!(
                method_result.contains(expected_op),
                "expected '{expected_op}' in '{method_result}'"
            );
        }
    }

    #[test]
    fn like_encodes_percent_wildcards() {
        let p = path(client().from("t").select("*").like("name", "%ace%"));
        // `%` → `%25` by urlencoding
        assert!(p.contains("name=like."), "path={p}");
        assert!(p.contains("%25ace%25"), "expected encoded %% wildcards in path={p}");
    }

    #[test]
    fn ilike_is_case_insensitive_op() {
        let p = path(client().from("t").select("*").ilike("email", "%EXAMPLE%"));
        assert!(p.contains("email=ilike."), "path={p}");
    }

    #[test]
    fn is_null_filter() {
        let p = path(client().from("t").select("*").is("deleted_at", "null"));
        assert_eq!(p, "/rest/v1/t?select=%2A&deleted_at=is.null");
    }

    #[test]
    fn is_true_filter() {
        let p = path(client().from("t").select("*").is("verified", "true"));
        assert_eq!(p, "/rest/v1/t?select=%2A&verified=is.true");
    }

    #[test]
    fn in_strings() {
        let p = path(client().from("t").select("*").in_("role", ["admin", "user"]));
        assert!(p.contains("role=in."), "path={p}");
        assert!(p.contains("admin"), "path={p}");
        assert!(p.contains("user"), "path={p}");
    }

    #[test]
    fn contains_cs_op() {
        let p = path(client().from("t").select("*").contains("tags", "{rust}"));
        assert!(p.contains("tags=cs."), "path={p}");
    }

    #[test]
    fn contained_by_cd_op() {
        let p = path(client().from("t").select("*").contained_by("tags", "{a,b}"));
        assert!(p.contains("tags=cd."), "path={p}");
    }

    #[test]
    fn overlaps_ov_op() {
        let p = path(client().from("t").select("*").overlaps("nums", "{1,2}"));
        assert!(p.contains("nums=ov."), "path={p}");
    }

    #[test]
    fn range_ops_use_correct_postgrest_ops() {
        let cases = [
            (path(client().from("t").select("*").range_lt("r", "(0,5)")), "r=sl."),
            (path(client().from("t").select("*").range_gt("r", "(0,5)")), "r=sr."),
            (path(client().from("t").select("*").range_lte("r", "(0,5)")), "r=nxr."),
            (path(client().from("t").select("*").range_gte("r", "(0,5)")), "r=nxl."),
            (path(client().from("t").select("*").range_adjacent("r", "(0,5)")), "r=adj."),
        ];
        for (p, expected) in cases {
            assert!(p.contains(expected), "expected '{expected}' in '{p}'");
        }
    }

    #[test]
    fn match_with_multiple_keys() {
        let p = path(client().from("t").select("*")
            .match_(serde_json::json!({"a": "1", "b": "2"})));
        assert!(p.contains("a=eq.1"), "path={p}");
        assert!(p.contains("b=eq.2"), "path={p}");
    }

    #[test]
    fn match_renders_non_string_values_via_to_string() {
        // Exercises the `other => other.to_string()` arm in match_.
        let p = path(
            client()
                .from("t")
                .select("*")
                .match_(serde_json::json!({"n": 42, "ok": true})),
        );
        // Numbers render bare; bools as `true`/`false`.
        assert!(p.contains("n=eq.42"), "path={p}");
        assert!(p.contains("ok=eq.true"), "path={p}");
    }

    #[test]
    fn match_ignores_non_object_value() {
        // The early-return path when the input isn't a JSON object.
        let p = path(
            client()
                .from("t")
                .select("*")
                .match_(serde_json::json!(["not", "an", "object"])),
        );
        // No filter params should have been appended.
        assert_eq!(p, "/rest/v1/t?select=%2A");
    }

    #[test]
    fn not_wraps_op_correctly() {
        let p = path(client().from("t").select("*").not("status", "eq", "banned"));
        assert_eq!(p, "/rest/v1/t?select=%2A&status=not.eq.banned");
    }

    #[test]
    fn not_with_in_op() {
        let p = path(client().from("t").select("*").not("id", "in", "(1,2,3)"));
        assert!(p.contains("id=not.in."), "path={p}");
    }

    #[test]
    fn or_wraps_in_parentheses() {
        let p = path(client().from("t").select("*").or("a.eq.1,b.eq.2"));
        assert!(p.contains("or="), "path={p}");
        // The value should be wrapped: (a.eq.1,b.eq.2)
        assert!(p.contains("%28"), "expected encoded '(' in path={p}");
    }

    #[test]
    fn filter_escape_hatch_any_op() {
        let p = path(client().from("t").select("*").filter("col", "eq", "val"));
        assert!(p.contains("col=eq.val"), "path={p}");
    }

    #[test]
    fn chaining_many_filters() {
        let p = path(
            client()
                .from("t")
                .select("*")
                .eq("a", "1")
                .neq("b", "2")
                .gt("c", 3)
                .lt("d", 4)
                .is("e", "null"),
        );
        assert!(p.contains("a=eq.1"), "path={p}");
        assert!(p.contains("b=neq.2"), "path={p}");
        assert!(p.contains("c=gt.3"), "path={p}");
        assert!(p.contains("d=lt.4"), "path={p}");
        assert!(p.contains("e=is.null"), "path={p}");
    }
}

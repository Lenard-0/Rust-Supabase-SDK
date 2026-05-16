use serde_json::Value;
use urlencoding::encode;

use crate::error::Result;
use crate::universals::{HttpMethod, RequestOptions};
use crate::SupabaseClient;

/// Represents a filter operator.
#[derive(Debug, Clone, PartialEq)]
pub enum Operator {
    Eq,
    Neq,
    Gt,
    Lt,
    Gte,
    Lte,
    Like,
}

impl Operator {
    /// Returns the string representation of the operator.
    pub fn as_str(&self) -> &'static str {
        match self {
            Operator::Eq => "eq",
            Operator::Neq => "neq",
            Operator::Gt => "gt",
            Operator::Lt => "lt",
            Operator::Gte => "gte",
            Operator::Lte => "lte",
            Operator::Like => "like",
        }
    }
}

/// Represents a single filter condition.
#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    pub column: String,
    pub operator: Operator,
    pub value: String,
}

impl Filter {
    /// Creates a new filter.
    pub fn new(column: &str, operator: Operator, value: &str) -> Self {
        Self {
            column: column.to_string(),
            operator,
            value: value.to_string(),
        }
    }

    /// Returns the query string for this filter (used for AND groups).
    /// Example: `name=eq.Org%20X`
    pub fn to_query(&self) -> String {
        format!(
            "{}={}.{}",
            encode(&self.column),
            self.operator.as_str(),
            encode(&self.value)
        )
    }

    /// Returns the query string for this filter when used in an OR group.
    /// Example: `name.eq.Org%20X`
    pub fn to_or_query(&self) -> String {
        format!(
            "{}.{}.{}",
            encode(&self.column),
            self.operator.as_str(),
            encode(&self.value)
        )
    }
}

/// Represents logical operators for grouping filters.
#[derive(Debug, Clone, PartialEq)]
pub enum LogicalOperator {
    And,
    Or,
}

/// Represents a group of filters combined by a logical operator.
/// For OR groups, PostgREST expects a query parameter in the form:
/// `or=(filter1,filter2,...)` where each filter is formatted as `column.operator.value`.
/// For AND groups, filters are concatenated as separate query parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct FilterGroup {
    pub operator: LogicalOperator,
    pub filters: Vec<Filter>,
}

impl FilterGroup {
    /// Creates a new filter group.
    pub fn new(operator: LogicalOperator, filters: Vec<Filter>) -> Self {
        Self { operator, filters }
    }

    /// Returns the query string for the filter group.
    pub fn to_query_string(&self) -> String {
        match self.operator {
            LogicalOperator::Or => {
                let inner: Vec<String> = self.filters.iter().map(|f| f.to_or_query()).collect();
                format!("or=({})", inner.join(","))
            }
            LogicalOperator::And => {
                let inner: Vec<String> = self.filters.iter().map(|f| f.to_query()).collect();
                inner.join("&")
            }
        }
    }
}

/// Represents the sort direction.
#[derive(Debug, Clone, PartialEq)]
pub enum SortDirection {
    Asc,
    Desc,
}

impl SortDirection {
    /// Returns the string representation of the sort direction.
    pub fn as_str(&self) -> &'static str {
        match self {
            SortDirection::Asc => "asc",
            SortDirection::Desc => "desc",
        }
    }
}

/// Represents an ordering on a column.
#[derive(Debug, Clone, PartialEq)]
pub struct Sort {
    pub column: String,
    pub direction: SortDirection,
}

impl Sort {
    /// Creates a new sort.
    pub fn new(column: &str, direction: SortDirection) -> Self {
        Self {
            column: column.to_string(),
            direction,
        }
    }

    /// Returns the query string for the sort.
    /// Example: `order=created_at.desc`
    pub fn to_query(&self) -> String {
        format!("order={}.{}", self.column, self.direction.as_str())
    }
}

/// Combines an optional filter group and sorts into a complete select query.
/// The query string always begins with `select=<columns>` (currently hardcoded to select all).
#[derive(Debug, Clone, PartialEq)]
pub struct SelectQuery {
    /// A single filter group. Even a single filter should be wrapped in a group (e.g., an AND group).
    pub filter: Option<FilterGroup>,
    pub sorts: Vec<Sort>,
}

impl Default for SelectQuery {
    fn default() -> Self {
        Self::new()
    }
}

impl SelectQuery {
    /// Creates a new empty select query.
    pub fn new() -> Self {
        Self {
            filter: None,
            sorts: Vec::new(),
        }
    }

    /// Returns the complete query string.
    ///
    /// For example:
    /// ```text
    /// select=%2A&name=eq.Test%20Organisation&id=eq.123&order=created_at.asc
    /// ```
    pub fn to_query_string(&self) -> String {
        let mut parts = vec![format!("select={}", encode("*"))];
        if let Some(ref group) = self.filter {
            parts.push(group.to_query_string());
        }
        for sort in &self.sorts {
            parts.push(sort.to_query());
        }
        parts.join("&")
    }

    pub fn sort(mut self, column: &str, direction: SortDirection) -> Self {
        self.sorts.push(Sort::new(column, direction));
        self
    }
}

impl SupabaseClient {
    /// Select rows from the specified table using a constructed `SelectQuery`.
    ///
    /// **Deprecated:** prefer the chainable builder
    /// [`client.from(table).select("*")...`](crate::postgrest::TableBuilder::select).
    #[deprecated(since = "0.3.0", note = "use `client.from(table).select(...).filter(...)`")]
    pub async fn select(&self, table_name: &str, query: SelectQuery) -> Result<Vec<Value>> {
        let path = format!("/rest/v1/{}?{}", table_name, query.to_query_string());

        let value = self
            .request_with(&path, HttpMethod::Get, None, &RequestOptions::postgrest())
            .await?;

        match value {
            Value::Array(arr) => Ok(arr),
            Value::Null => Ok(Vec::new()),
            other => Ok(vec![other]),
        }
    }
}

// --- DSL for building queries with operators ---

use std::ops::{BitAnd, BitOr};

/// A newtype for a field/column name.
#[derive(Debug, Clone, PartialEq)]
pub struct Field(pub String);

impl Field {
    pub fn new(s: &str) -> Self {
        Field(s.to_string())
    }

    pub fn eq(self, value: impl ToString) -> Query {
        Query::Condition(QueryExpr {
            column: self.0,
            operator: Operator::Eq,
            value: value.to_string(),
        })
    }

    pub fn neq(self, value: impl ToString) -> Query {
        Query::Condition(QueryExpr {
            column: self.0,
            operator: Operator::Neq,
            value: value.to_string(),
        })
    }

    pub fn gt(self, value: impl ToString) -> Query {
        Query::Condition(QueryExpr {
            column: self.0,
            operator: Operator::Gt,
            value: value.to_string(),
        })
    }

    pub fn lt(self, value: impl ToString) -> Query {
        Query::Condition(QueryExpr {
            column: self.0,
            operator: Operator::Lt,
            value: value.to_string(),
        })
    }

    pub fn gte(self, value: impl ToString) -> Query {
        Query::Condition(QueryExpr {
            column: self.0,
            operator: Operator::Gte,
            value: value.to_string(),
        })
    }

    pub fn lte(self, value: impl ToString) -> Query {
        Query::Condition(QueryExpr {
            column: self.0,
            operator: Operator::Lte,
            value: value.to_string(),
        })
    }

    pub fn like(self, value: impl ToString) -> Query {
        Query::Condition(QueryExpr {
            column: self.0,
            operator: Operator::Like,
            value: value.to_string(),
        })
    }
}

/// Represents a basic query expression (a single condition).
#[derive(Debug, Clone, PartialEq)]
pub struct QueryExpr {
    pub column: String,
    pub operator: Operator,
    pub value: String,
}

impl QueryExpr {
    pub fn to_filter(&self) -> Filter {
        Filter::new(&self.column, self.operator.clone(), &self.value)
    }
}

/// Represents a DSL query expression which can be a condition or a combination (AND/OR).
#[derive(Debug, Clone, PartialEq)]
pub enum Query {
    Condition(QueryExpr),
    And(Box<Query>, Box<Query>),
    Or(Box<Query>, Box<Query>),
}

impl Query {
    pub fn to_query(&self) -> SelectQuery {
        let filter_group = self.to_filter_group();
        SelectQuery {
            filter: Some(filter_group),
            sorts: Vec::new(),
        }
    }

    /// Convert a Query tree into a FilterGroup.
    pub fn to_filter_group(&self) -> FilterGroup {
        match self {
            Query::Condition(expr) => FilterGroup::new(LogicalOperator::And, vec![expr.to_filter()]),
            Query::And(left, right) => {
                let mut filters = left.to_filter_group().filters;
                filters.extend(right.to_filter_group().filters);
                FilterGroup::new(LogicalOperator::And, filters)
            }
            Query::Or(left, right) => {
                let mut filters = left.to_filter_group().filters;
                filters.extend(right.to_filter_group().filters);
                FilterGroup::new(LogicalOperator::Or, filters)
            }
        }
    }
}

impl BitAnd for Query {
    type Output = Query;
    fn bitand(self, rhs: Query) -> Query {
        Query::And(Box::new(self), Box::new(rhs))
    }
}

impl BitOr for Query {
    type Output = Query;
    fn bitor(self, rhs: Query) -> Query {
        Query::Or(Box::new(self), Box::new(rhs))
    }
}

/// Macro to allow writing queries with a natural syntax.
/// Example:
///     let expr = q!("name" == "Org X") | q!("category" != "Finance") & q!("property" > 5);
#[macro_export]
macro_rules! query {
    ($col:tt == $val:expr) => {
        $crate::select::Field::new($col).eq($val)
    };
    ($col:tt != $val:expr) => {
        $crate::select::Field::new($col).neq($val)
    };
    ($col:tt > $val:expr) => {
        $crate::select::Field::new($col).gt($val)
    };
    ($col:tt < $val:expr) => {
        $crate::select::Field::new($col).lt($val)
    };
    ($col:tt >= $val:expr) => {
        $crate::select::Field::new($col).gte($val)
    };
    ($col:tt <= $val:expr) => {
        $crate::select::Field::new($col).lte($val)
    };
    // Support grouping with & (AND)
    ($left:tt & $right:tt) => {
        ($crate::select::q!($left)) & ($crate::select::q!($right))
    };
    // Support grouping with | (OR)
    ($left:tt | $right:tt) => {
        ($crate::select::q!($left)) | ($crate::select::q!($right))
    };
    // Parentheses support.
    ( ( $($inner:tt)+ ) ) => {
        $crate::select::q!($($inner)+)
    };
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // --- Operator::as_str ---

    #[test]
    fn operator_as_str_all_variants() {
        assert_eq!(Operator::Eq.as_str(), "eq");
        assert_eq!(Operator::Neq.as_str(), "neq");
        assert_eq!(Operator::Gt.as_str(), "gt");
        assert_eq!(Operator::Lt.as_str(), "lt");
        assert_eq!(Operator::Gte.as_str(), "gte");
        assert_eq!(Operator::Lte.as_str(), "lte");
        assert_eq!(Operator::Like.as_str(), "like");
    }

    // --- Filter ---

    #[test]
    fn filter_to_query_plain() {
        let f = Filter::new("name", Operator::Eq, "Alice");
        assert_eq!(f.to_query(), "name=eq.Alice");
    }

    #[test]
    fn filter_to_query_encodes_spaces() {
        let f = Filter::new("name", Operator::Eq, "Org X");
        assert_eq!(f.to_query(), "name=eq.Org%20X");
    }

    #[test]
    fn filter_to_query_encodes_column() {
        let f = Filter::new("my col", Operator::Neq, "val");
        assert!(f.to_query().starts_with("my%20col="), "{}", f.to_query());
    }

    #[test]
    fn filter_to_or_query() {
        let f = Filter::new("id", Operator::Eq, "123");
        assert_eq!(f.to_or_query(), "id.eq.123");
    }

    #[test]
    fn filter_to_or_query_encodes_value() {
        let f = Filter::new("name", Operator::Like, "Org*");
        assert_eq!(f.to_or_query(), "name.like.Org%2A");
    }

    // --- FilterGroup ---

    #[test]
    fn filter_group_and_joins_with_ampersand() {
        let filters = vec![
            Filter::new("a", Operator::Eq, "1"),
            Filter::new("b", Operator::Eq, "2"),
        ];
        let g = FilterGroup::new(LogicalOperator::And, filters);
        let q = g.to_query_string();
        assert!(q.contains("a=eq.1"), "{q}");
        assert!(q.contains("b=eq.2"), "{q}");
        assert!(q.contains('&'), "{q}");
    }

    #[test]
    fn filter_group_or_wraps_in_or_parens() {
        let filters = vec![
            Filter::new("status", Operator::Eq, "active"),
            Filter::new("status", Operator::Eq, "trial"),
        ];
        let g = FilterGroup::new(LogicalOperator::Or, filters);
        let q = g.to_query_string();
        assert!(q.starts_with("or=("), "{q}");
        assert!(q.contains("status.eq.active"), "{q}");
        assert!(q.contains("status.eq.trial"), "{q}");
    }

    #[test]
    fn filter_group_empty_and_produces_empty_string() {
        let g = FilterGroup::new(LogicalOperator::And, vec![]);
        assert_eq!(g.to_query_string(), "");
    }

    // --- SortDirection ---

    #[test]
    fn sort_direction_as_str() {
        assert_eq!(SortDirection::Asc.as_str(), "asc");
        assert_eq!(SortDirection::Desc.as_str(), "desc");
    }

    // --- Sort ---

    #[test]
    fn sort_to_query() {
        let s = Sort::new("created_at", SortDirection::Desc);
        assert_eq!(s.to_query(), "order=created_at.desc");
    }

    #[test]
    fn sort_to_query_asc() {
        assert_eq!(Sort::new("name", SortDirection::Asc).to_query(), "order=name.asc");
    }

    // --- SelectQuery ---

    #[test]
    fn select_query_default_has_only_select_star() {
        let q = SelectQuery::new().to_query_string();
        assert_eq!(q, "select=%2A");
    }

    #[test]
    fn select_query_with_and_filter() {
        let q = SelectQuery {
            filter: Some(FilterGroup::new(
                LogicalOperator::And,
                vec![Filter::new("id", Operator::Eq, "42")],
            )),
            sorts: Vec::new(),
        }
        .to_query_string();
        assert!(q.contains("id=eq.42"), "{q}");
        assert!(q.starts_with("select="), "{q}");
    }

    #[test]
    fn select_query_with_sort() {
        let q = SelectQuery::new()
            .sort("name", SortDirection::Asc)
            .to_query_string();
        assert!(q.contains("order=name.asc"), "{q}");
    }

    #[test]
    fn select_query_with_filter_and_sort() {
        let q = SelectQuery {
            filter: Some(FilterGroup::new(
                LogicalOperator::And,
                vec![Filter::new("status", Operator::Eq, "active")],
            )),
            sorts: vec![Sort::new("created_at", SortDirection::Desc)],
        }
        .to_query_string();
        assert!(q.contains("status=eq.active"), "{q}");
        assert!(q.contains("order=created_at.desc"), "{q}");
    }

    // --- Field builder ---

    #[test]
    fn field_eq_produces_condition() {
        let q = Field::new("score").eq(100).to_query();
        let qs = q.to_query_string();
        assert!(qs.contains("score=eq.100"), "{qs}");
    }

    #[test]
    fn field_neq() {
        let q = Field::new("status").neq("banned").to_query().to_query_string();
        assert!(q.contains("status=neq.banned"), "{q}");
    }

    #[test]
    fn field_gt_lt_gte_lte() {
        assert!(Field::new("x").gt(5).to_query().to_query_string().contains("x=gt.5"));
        assert!(Field::new("x").lt(5).to_query().to_query_string().contains("x=lt.5"));
        assert!(Field::new("x").gte(5).to_query().to_query_string().contains("x=gte.5"));
        assert!(Field::new("x").lte(5).to_query().to_query_string().contains("x=lte.5"));
    }

    #[test]
    fn field_like() {
        let q = Field::new("name").like("%alice%").to_query().to_query_string();
        assert!(q.contains("name=like."), "{q}");
    }

    // --- Query combinators ---

    #[test]
    fn query_bitand_produces_and_group() {
        let q = (Field::new("a").eq(1) & Field::new("b").eq(2))
            .to_filter_group();
        assert_eq!(q.operator, LogicalOperator::And);
        assert_eq!(q.filters.len(), 2);
    }

    #[test]
    fn query_bitor_produces_or_group() {
        let q = (Field::new("a").eq(1) | Field::new("b").eq(2))
            .to_filter_group();
        assert_eq!(q.operator, LogicalOperator::Or);
        assert_eq!(q.filters.len(), 2);
    }

    #[test]
    fn query_condition_to_filter_group_is_and_single() {
        let q = Field::new("id").eq("x").to_filter_group();
        assert_eq!(q.operator, LogicalOperator::And);
        assert_eq!(q.filters.len(), 1);
    }

    // --- query! macro ---

    #[test]
    fn query_macro_eq() {
        let q = query!("name" == "Alice");
        let qs = q.to_query().to_query_string();
        assert!(qs.contains("name=eq.Alice"), "{qs}");
    }

    #[test]
    fn query_macro_neq() {
        let q = query!("status" != "banned");
        let qs = q.to_query().to_query_string();
        assert!(qs.contains("status=neq.banned"), "{qs}");
    }

    #[test]
    fn query_macro_gt_lt() {
        let q = query!("score" > 90);
        assert!(q.to_query().to_query_string().contains("score=gt.90"));
        let q2 = query!("score" < 10);
        assert!(q2.to_query().to_query_string().contains("score=lt.10"));
    }

    #[test]
    fn query_macro_gte_lte() {
        assert!(query!("x" >= 5).to_query().to_query_string().contains("x=gte.5"));
        assert!(query!("x" <= 5).to_query().to_query_string().contains("x=lte.5"));
    }

    #[test]
    fn select_query_default_matches_new() {
        let d = SelectQuery::default();
        let n = SelectQuery::new();
        assert_eq!(d.filter, n.filter);
        assert_eq!(d.sorts.len(), n.sorts.len());
    }
}


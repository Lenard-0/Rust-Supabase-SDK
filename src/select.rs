use reqwest::{self, header::HeaderValue, Client};
use serde_json::Value;
use urlencoding::encode;
use url::Url;

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
    /// ```
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
    /// This method builds the query string from the filter group and sorts defined in the `SelectQuery` struct.
    pub async fn select(&self, table_name: &str, query: SelectQuery) -> Result<Vec<Value>, String> {
        let mut url = Url::parse(&format!("{}/rest/v1/{}", self.url, table_name))
            .map_err(|e| e.to_string())?;
        let query_string = query.to_query_string();
        url.set_query(Some(&query_string));

        let client = Client::new();
        let response = client
            .get(url)
            .header("apikey", HeaderValue::from_str(&self.api_key).map_err(|e| e.to_string())?)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("Request failed with status: {}", response.status()));
        }
        let json: Vec<Value> = response.json().await.map_err(|e| e.to_string())?;
        Ok(json)
    }
}

/// --- DSL for building queries with operators --- ///

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


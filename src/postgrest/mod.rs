//! PostgREST query builder — the primary data API.
//!
//! Mirrors the chainable interface of `@supabase/postgrest-js`:
//!
//! ```no_run
//! # use rust_supabase_sdk::SupabaseClient;
//! # async fn demo(client: &SupabaseClient) -> rust_supabase_sdk::Result<()> {
//! let rows: Vec<serde_json::Value> = client
//!     .from("countries")
//!     .select("*")
//!     .eq("status", "active")
//!     .gt("population", 1_000_000)
//!     .order("created_at", false)
//!     .limit(10)
//!     .await?;
//! # Ok(()) }
//! ```

mod builder;
mod column;
mod filters;
pub mod row;
mod typed_builder;
mod value;

pub use builder::{
    CountMode, MaybeSingleBuilder, Order, PostgrestBuilder, SingleBuilder, TableBuilder,
    TextSearchType,
};
pub use column::{Column, IntoColumnName};
pub use row::Row;
pub use typed_builder::TypedBuilder;
pub use value::PostgrestValue;

use crate::SupabaseClient;

impl SupabaseClient {
    /// Open a query builder against the given table.
    pub fn from(&self, table: impl Into<String>) -> TableBuilder {
        TableBuilder::new(self.clone(), table.into())
    }

    /// Open a **type-safe** query builder against the table bound to row
    /// type `R`. Returns a [`TypedBuilder<R>`] — filter values are checked
    /// against each column's declared Rust type at compile time.
    ///
    /// Pairs with the [`Row`] trait and the codegen-emitted `Column<R, _>`
    /// constants (see [`Column`](crate::postgrest::Column)).
    ///
    /// ```no_run
    /// # use rust_supabase_sdk::{SupabaseClient, Row, postgrest::Column};
    /// # use serde::{Serialize, Deserialize};
    /// #[derive(Debug, Clone, Serialize, Deserialize)]
    /// struct User { id: String, email: String }
    /// impl Row for User { const TABLE: &'static str = "users"; }
    ///
    /// #[allow(non_upper_case_globals)]
    /// impl User {
    ///     pub const email: Column<User, String> = Column::new("email");
    /// }
    ///
    /// # async fn demo(c: SupabaseClient) -> rust_supabase_sdk::Result<()> {
    /// let users: Vec<User> = c.from_row::<User>()
    ///     .eq(User::email, "alice@example.com".to_string())
    ///     .execute()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    ///
    /// For ad-hoc / stringly-typed queries (or queries against views or
    /// columns that codegen didn't emit), use
    /// [`SupabaseClient::from`](crate::SupabaseClient::from) instead.
    pub fn from_row<R: Row>(&self) -> TypedBuilder<R> {
        TypedBuilder::from_table(TableBuilder::new(self.clone(), R::TABLE.into()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Widget {
        id: String,
    }

    impl Row for Widget {
        const TABLE: &'static str = "widgets";
    }

    #[test]
    fn from_row_uses_row_table_constant() {
        let client = SupabaseClient::new("https://x.supabase.co", "anon", None);
        let path = client.from_row::<Widget>().build_path();
        // TypedBuilder defaults to `select=*` upon construction.
        assert_eq!(path, "/rest/v1/widgets?select=%2A");
    }
}

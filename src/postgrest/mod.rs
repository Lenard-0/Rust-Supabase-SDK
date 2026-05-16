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
mod filters;
pub mod row;
mod value;

pub use builder::{
    CountMode, MaybeSingleBuilder, Order, PostgrestBuilder, SingleBuilder, TableBuilder,
    TextSearchType,
};
pub use row::Row;
pub use value::PostgrestValue;

use crate::SupabaseClient;

impl SupabaseClient {
    /// Open a query builder against the given table.
    pub fn from(&self, table: impl Into<String>) -> TableBuilder {
        TableBuilder::new(self.clone(), table.into())
    }

    /// Open a query builder against the table bound to row type `R`.
    ///
    /// Pairs with the [`Row`] trait — typically implemented on structs
    /// emitted by `cargo supabase gen types`.
    ///
    /// ```no_run
    /// # use rust_supabase_sdk::{SupabaseClient, Row};
    /// # use serde::{Serialize, Deserialize};
    /// #[derive(Serialize, Deserialize)]
    /// struct User { id: String, email: String }
    /// impl Row for User { const TABLE: &'static str = "users"; }
    ///
    /// # async fn demo(c: SupabaseClient) -> rust_supabase_sdk::Result<()> {
    /// let users: Vec<serde_json::Value> = c.from_row::<User>().select("*").await?;
    /// # Ok(()) }
    /// ```
    pub fn from_row<R: Row>(&self) -> TableBuilder {
        TableBuilder::new(self.clone(), R::TABLE.into())
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
        let path = client.from_row::<Widget>().select("*").build_path();
        assert_eq!(path, "/rest/v1/widgets?select=%2A");
    }
}

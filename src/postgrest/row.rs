//! Type-level binding between a Rust struct and a PostgREST table.
//!
//! Implementing [`Row`] lets you use [`SupabaseClient::from_row`](crate::SupabaseClient::from_row)
//! and skip the stringly-typed table name:
//!
//! ```
//! use rust_supabase_sdk::Row;
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Debug, Clone, Serialize, Deserialize)]
//! pub struct Country {
//!     pub id: i64,
//!     pub name: String,
//! }
//!
//! impl Row for Country {
//!     const TABLE: &'static str = "countries";
//! }
//! ```
//!
//! `cargo supabase gen types` emits this `impl` automatically for every
//! discovered table.

use serde::de::DeserializeOwned;
use serde::Serialize;

/// A Rust type bound to a PostgREST table. Implementors are usable as the
/// `R` parameter in [`SupabaseClient::from_row`](crate::SupabaseClient::from_row).
///
/// The trait is intentionally minimal — it only carries the table name today.
/// Future minor versions may add associated columns / primary-key metadata
/// behind default-defaulted associated items so existing impls keep compiling.
pub trait Row: DeserializeOwned + Serialize + Send + Sync + 'static {
    /// The PostgREST/PostgreSQL table name. Forwarded to
    /// [`SupabaseClient::from`](crate::SupabaseClient::from) when this row
    /// type is used as a generic parameter.
    const TABLE: &'static str;

    /// Optional list of column names. Defaults to `&[]` for hand-written
    /// impls; codegen fills this in. Treat as advisory — the database is the
    /// source of truth.
    const COLUMNS: &'static [&'static str] = &[];

    /// Optional schema. Defaults to `None` meaning the client's configured
    /// schema (typically `public`).
    const SCHEMA: Option<&'static str> = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Country {
        id: i64,
        name: String,
    }

    impl Row for Country {
        const TABLE: &'static str = "countries";
        const COLUMNS: &'static [&'static str] = &["id", "name"];
    }

    #[test]
    fn row_metadata_is_reachable() {
        assert_eq!(Country::TABLE, "countries");
        assert_eq!(Country::COLUMNS, &["id", "name"]);
        assert_eq!(Country::SCHEMA, None);
    }
}

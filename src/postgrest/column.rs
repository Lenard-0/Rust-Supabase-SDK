//! Compile-time-checked column references.
//!
//! A [`Column<R, V>`] is a zero-sized handle bound to a row type `R` and a
//! value type `V`. It exists to make the typed query path
//! ([`SupabaseClient::from_row`](crate::SupabaseClient::from_row)) catch
//! drift between your Rust types and your Postgres schema at compile time:
//!
//! ```rust,ignore
//! // After `cargo supabase gen types`, each table struct gets a constants
//! // block emitted alongside its `Row` impl:
//! use rust_supabase_sdk::postgrest::Column;
//!
//! #[derive(serde::Deserialize, serde::Serialize)]
//! pub struct Posts { /* ... */ }
//!
//! #[allow(non_upper_case_globals)]
//! impl Posts {
//!     pub const id:         Column<Posts, String>          = Column::new("id");
//!     pub const status:     Column<Posts, String>          = Column::new("status");
//!     pub const view_count: Column<Posts, i32>             = Column::new("view_count");
//!     pub const archived:   Column<Posts, Option<bool>>    = Column::new("archived");
//! }
//! ```
//!
//! On the call site:
//!
//! ```rust,ignore
//! client.from_row::<Posts>()
//!     .select("*")
//!     .eq(Posts::status, "published")     // ✓ Column<Posts, String> matches &str
//!     .gt(Posts::view_count, 100i32)      // ✓ i32 matches
//!     // .eq(Posts::view_count, "abc")    // ✗ compile error: i32 ≠ &str
//!     // .eq(Users::id, "x")              // ✗ compile error: wrong table
//!     .is_null(Posts::archived)           // ✓ archived is Option<_>
//!     // .is_null(Posts::status)          // ✗ status is non-nullable
//!     .execute().await?;
//! ```
//!
//! The runtime cost is zero: `Column<R, V>` carries only a `&'static str`
//! plus a phantom type. Codegen emits one `const` per database column.

use std::fmt;
use std::marker::PhantomData;

/// A statically-typed reference to a column in row type `R` with value type `V`.
///
/// Construct via [`Column::new`] — codegen does this for every column it
/// discovers. The `fn(R) -> V` phantom marker is correct: contravariant in
/// the row type (you can't use a `posts` column where a stricter row type is
/// expected) and covariant in the value type (a column whose value coerces
/// to `V` can stand in for a `Column<_, V>`).
pub struct Column<R, V> {
    name: &'static str,
    _phantom: PhantomData<fn(R) -> V>,
}

impl<R, V> Column<R, V> {
    /// Construct a typed column reference. Intended for use from codegen-
    /// emitted `const` items; hand-written impls are rare but valid.
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            _phantom: PhantomData,
        }
    }

    /// The column name as it appears in PostgREST URL parameters.
    pub const fn name(&self) -> &'static str {
        self.name
    }
}

// Manual impls so `R` and `V` don't need bounds.
impl<R, V> Clone for Column<R, V> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<R, V> Copy for Column<R, V> {}

impl<R, V> fmt::Debug for Column<R, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Column").field("name", &self.name).finish()
    }
}

impl<R, V> PartialEq for Column<R, V> {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}
impl<R, V> Eq for Column<R, V> {}

/// Anything that resolves to a column name for a row type `R`.
///
/// Used by methods where value-type checking isn't valuable (e.g. ordering,
/// where the column is just a name string). Both raw string slices and
/// typed [`Column<R, _>`] handles satisfy this — string lets you reference
/// a column that codegen didn't emit (a view, a function result), typed lets
/// you stay inside the compile-time-checked world.
pub trait IntoColumnName<R> {
    fn into_column_name(self) -> String;
}

impl<R, V> IntoColumnName<R> for Column<R, V> {
    fn into_column_name(self) -> String {
        self.name.to_string()
    }
}

impl<R> IntoColumnName<R> for &str {
    fn into_column_name(self) -> String {
        self.to_string()
    }
}

impl<R> IntoColumnName<R> for String {
    fn into_column_name(self) -> String {
        self
    }
}

impl<R> IntoColumnName<R> for &String {
    fn into_column_name(self) -> String {
        self.clone()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // Fake row types to exercise the type parameter without needing the
    // full `Row` trait. `Users` is only used as a *type* parameter — we
    // never instantiate one, hence the dead-code allow.
    struct Posts;
    #[allow(dead_code)]
    struct Users;

    #[test]
    fn column_name_round_trips() {
        let c: Column<Posts, String> = Column::new("status");
        assert_eq!(c.name(), "status");
    }

    #[test]
    fn column_is_copy_and_clone() {
        let c: Column<Posts, i32> = Column::new("view_count");
        let copied = c;
        // Explicit clone() to prove the trait impl exists, even though Copy
        // makes it strictly unnecessary.
        #[allow(clippy::clone_on_copy)]
        let cloned = c.clone();
        assert_eq!(copied.name(), "view_count");
        assert_eq!(cloned.name(), "view_count");
        // Original is still usable because Column is Copy.
        assert_eq!(c.name(), "view_count");
    }

    #[test]
    fn column_debug_does_not_panic() {
        let c: Column<Posts, String> = Column::new("id");
        let s = format!("{c:?}");
        assert!(s.contains("Column"), "{s}");
        assert!(s.contains("id"), "{s}");
    }

    #[test]
    fn column_partial_eq_compares_by_name_only() {
        let a: Column<Posts, String> = Column::new("id");
        let b: Column<Posts, String> = Column::new("id");
        let c: Column<Posts, String> = Column::new("name");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    // The point of the type parameters is that the *compiler* keeps Posts::id
    // and Users::id apart. We can't directly assert "this fails to compile"
    // inside a unit test — trybuild covers that. What we *can* assert is that
    // two same-name columns from different row types have different *types*.
    // The fact that this file compiles is the proof.
    fn _type_parameters_are_distinct() {
        let _p: Column<Posts, String> = Column::new("id");
        let _u: Column<Users, String> = Column::new("id");
        // Uncommenting the next line would (correctly) fail to compile:
        //   let _p2: Column<Posts, String> = _u;
    }

    #[test]
    fn into_column_name_for_typed_column() {
        let c: Column<Posts, i32> = Column::new("view_count");
        assert_eq!(IntoColumnName::<Posts>::into_column_name(c), "view_count");
    }

    #[test]
    fn into_column_name_for_str() {
        let s: &str = "raw_column";
        assert_eq!(IntoColumnName::<Posts>::into_column_name(s), "raw_column");
    }

    #[test]
    fn into_column_name_for_string() {
        let s = String::from("owned_column");
        assert_eq!(
            IntoColumnName::<Posts>::into_column_name(s),
            "owned_column"
        );
    }

    #[test]
    fn into_column_name_for_string_ref() {
        let s = String::from("ref_column");
        assert_eq!(
            IntoColumnName::<Posts>::into_column_name(&s),
            "ref_column"
        );
    }
}

//! Misuse: `.in_` with iterator elements that don't match the column type.
//!
//! Signature is `fn in_<V, I>(self, col: Column<R, V>, vals: I) where I:
//! IntoIterator<Item = V>` — so `Column<Posts, i32>` + `["a", "b"]` is a
//! mismatch between `V = i32` and `Item = &str`.

use rust_supabase_sdk::{postgrest::Column, Row, SupabaseClient};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Posts {
    id: String,
    status: String,
    view_count: i32,
    archived: Option<bool>,
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
}

fn main() {
    let client = SupabaseClient::new("https://x", "k", None);
    let _ = client
        .from_row::<Posts>()
        .in_(Posts::view_count, ["a", "b"]);
}

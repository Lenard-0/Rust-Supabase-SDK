//! Misuse: `.is_null` on a non-nullable column.
//!
//! `is_null` requires `Column<R, Option<V>>`. `Posts::status` is
//! `Column<Posts, String>` (no `Option`), so the call must be rejected.

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
        .is_null(Posts::status);
}

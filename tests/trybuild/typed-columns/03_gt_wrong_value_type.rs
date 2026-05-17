//! Misuse: `.gt` with a value type that doesn't match the column.
//!
//! Same constraint as `.eq` — `V` is tied between column and value.

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
        .gt(Posts::view_count, "abc");
}

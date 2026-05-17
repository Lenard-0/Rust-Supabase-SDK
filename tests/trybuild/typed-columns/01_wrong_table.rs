//! Misuse: passing a `Column<Users, _>` into a `TypedBuilder<Posts>` chain.
//!
//! `.eq` is `fn eq<V>(self, col: Column<R, V>, val: V)` where `R` is the
//! builder's row type, so `Users::id` cannot stand in for a `Posts` column.

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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Users {
    id: String,
}
impl Row for Users {
    const TABLE: &'static str = "users";
}
#[allow(non_upper_case_globals)]
impl Users {
    pub const id: Column<Users, String> = Column::new("id");
}

fn main() {
    let client = SupabaseClient::new("https://x", "k", None);
    let _ = client
        .from_row::<Posts>()
        .eq(Users::id, "x".to_string());
}

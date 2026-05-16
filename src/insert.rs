use serde_json::{json, Value};

use crate::error::Result;
use crate::{generate_id, universals::HttpMethod, SupabaseClient};

impl SupabaseClient {
    /// Insert a row, auto-generating a UUID v4 for the `id` column. Returns the new id.
    ///
    /// **Deprecated:** the auto-UUID injection is a footgun. Prefer
    /// [`client.from(table).insert(...)`](crate::postgrest::TableBuilder::insert)
    /// which does not modify the body.
    #[deprecated(
        since = "0.3.0",
        note = "auto-UUID injection is implicit; use `client.from(table).insert(value)` instead"
    )]
    pub async fn insert(&self, table_name: &str, mut body: Value) -> Result<String> {
        let new_id = generate_id();
        body["id"] = json!(new_id);

        self.request(
            &format!("/rest/v1/{table_name}"),
            HttpMethod::Post,
            Some(body),
            false,
        )
        .await?;

        Ok(new_id)
    }
}

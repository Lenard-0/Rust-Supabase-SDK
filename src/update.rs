use serde_json::json;

use crate::error::Result;
use crate::{generate_id, universals::HttpMethod, SupabaseClient};

impl SupabaseClient {
    /// PATCH the row identified by `id` with the fields present in `body`.
    ///
    /// **Deprecated:** prefer
    /// [`client.from(table).update(body).eq("id", id)`](crate::postgrest::TableBuilder::update)
    /// which supports any filter expression.
    #[deprecated(since = "0.3.0", note = "use `client.from(table).update(body).eq(\"id\", id)`")]
    pub async fn update(&self, table_name: &str, id: &str, body: serde_json::Value) -> Result<()> {
        self.request(
            &format!("/rest/v1/{table_name}?id=eq.{id}"),
            HttpMethod::Patch,
            Some(body),
            false,
        )
        .await?;
        Ok(())
    }

    /// Insert if absent, update if present. Uses `id` as the conflict column; auto-generates
    /// one when not supplied. Returns the `id` used.
    ///
    /// **Deprecated:** prefer
    /// [`client.from(table).upsert(body).on_conflict("id")`](crate::postgrest::TableBuilder::upsert)
    /// which does not inject an `id` and lets you choose the conflict column.
    #[deprecated(
        since = "0.3.0",
        note = "use `client.from(table).upsert(body).on_conflict(\"id\")`"
    )]
    pub async fn upsert(
        &self,
        table_name: &str,
        mut body: serde_json::Value,
    ) -> Result<String> {
        let id = match body["id"].as_str() {
            Some(id) => id.to_string(),
            None => generate_id(),
        };
        body["id"] = json!(id);

        self.request(
            &format!("/rest/v1/{table_name}"),
            HttpMethod::Post,
            Some(body),
            true,
        )
        .await?;

        Ok(id)
    }
}

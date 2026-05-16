use crate::error::Result;
use crate::{universals::HttpMethod, SupabaseClient};

impl SupabaseClient {
    /// Delete the row whose `id` matches the given value.
    ///
    /// **Deprecated:** prefer the chainable builder
    /// [`client.from(table).delete().eq("id", id)`](crate::postgrest::TableBuilder::delete)
    /// which supports any filter expression.
    #[deprecated(since = "0.3.0", note = "use `client.from(table).delete().eq(\"id\", id)`")]
    pub async fn delete(&self, table_name: &str, id: &str) -> Result<()> {
        self.request(
            &format!("/rest/v1/{table_name}?id=eq.{id}"),
            HttpMethod::Delete,
            None,
            false,
        )
        .await?;
        Ok(())
    }
}

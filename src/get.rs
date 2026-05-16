use crate::error::{Result, SupabaseError};
use crate::{universals::HttpMethod, SupabaseClient};

impl SupabaseClient {
    /// Fetch a single row by `id`. Returns [`SupabaseError::NotFound`] if no row matches.
    ///
    /// **Deprecated:** prefer
    /// [`client.from(table).select("*").eq("id", id).single()`](crate::postgrest::TableBuilder::select).
    #[deprecated(
        since = "0.3.0",
        note = "use `client.from(table).select(\"*\").eq(\"id\", id).single()`"
    )]
    pub async fn get_by_id(&self, table_name: &str, id: &str) -> Result<serde_json::Value> {
        let result = self
            .request(
                &format!("/rest/v1/{table_name}?id=eq.{id}"),
                HttpMethod::Get,
                None,
                false,
            )
            .await?;

        match result.as_array() {
            Some(arr) if !arr.is_empty() => Ok(arr[0].clone()),
            _ => Err(SupabaseError::NotFound {
                resource: format!("{table_name}#{id}"),
            }),
        }
    }
}

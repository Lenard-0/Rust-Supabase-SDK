use crate::error::{Result, SupabaseError};
use crate::select::SelectQuery;
use crate::universals::{HttpMethod, RequestOptions};
use crate::SupabaseClient;

impl SupabaseClient {
    /// Return the number of rows matching the given query, parsed from the
    /// PostgREST `Content-Range` header (`Prefer: count=exact`).
    ///
    /// **Deprecated:** prefer
    /// `client.from(table).select("*").count(CountMode::Exact).execute_with_count()`
    /// which returns rows and count together.
    #[deprecated(
        since = "0.3.0",
        note = "use `client.from(table).select(\"*\").count(CountMode::Exact).execute_with_count()`"
    )]
    pub async fn count(&self, table_name: &str, query: SelectQuery) -> Result<usize> {
        let path = format!("/rest/v1/{}?{}", table_name, query.to_query_string());

        let opts = RequestOptions {
            prefer: vec!["count=exact".to_string()],
            ..RequestOptions::postgrest()
        };

        let (_status, headers, _body) = self
            .request_full(&path, HttpMethod::Get, None, &opts)
            .await?;

        let range = headers
            .get("content-range")
            .ok_or_else(|| SupabaseError::Unexpected("Missing Content-Range header".to_string()))?
            .to_str()
            .map_err(|e| SupabaseError::Unexpected(format!("Invalid Content-Range header: {e}")))?;

        let total = range.split('/').nth(1).ok_or_else(|| {
            SupabaseError::Unexpected(format!("Malformed Content-Range header: {range}"))
        })?;

        total
            .trim()
            .parse::<usize>()
            .map_err(|e| SupabaseError::Unexpected(format!("Failed to parse count `{total}`: {e}")))
    }
}

use crate::error::{Result, SupabaseError};
use crate::{universals::HttpMethod, SupabaseClient};

impl SupabaseClient {
    /// Call a Postgres function (`/rest/v1/rpc/<name>`) with a JSON-object body of args.
    /// Returns the response as a JSON array.
    pub async fn rpc_call(
        &self,
        rpc_name: &str,
        args: serde_json::Value,
    ) -> Result<Vec<serde_json::Value>> {
        let path = format!("/rest/v1/rpc/{rpc_name}");
        let response = self
            .request(&path, HttpMethod::Post, Some(args), false)
            .await?;

        match response {
            serde_json::Value::Array(arr) => Ok(arr),
            other => Err(SupabaseError::Unexpected(format!(
                "RPC `{rpc_name}` returned non-array value: {other}"
            ))),
        }
    }
}

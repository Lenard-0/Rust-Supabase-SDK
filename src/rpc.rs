use crate::{universals::HttpMethod, SupabaseClient};


impl SupabaseClient {
    /// Call a Supabase RPC with JSON input and get JSON array output.
    pub async fn rpc_call(
        &self,
        rpc_name: &str,
        args: serde_json::Value,  // JSON object with RPC args
    ) -> Result<Vec<serde_json::Value>, String> {
        let path = format!("/rest/v1/rpc/{}", rpc_name);

        let response = self
            .request(&path, HttpMethod::Post, Some(args), false)
            .await?;

        // Expect response to be a JSON array
        let arr = response
            .as_array()
            .ok_or_else(|| "Expected JSON array from RPC".to_string())?;

        Ok(arr.clone())
    }
}
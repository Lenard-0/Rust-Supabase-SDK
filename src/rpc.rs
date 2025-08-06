use serde::{de::DeserializeOwned, Serialize};
use crate::{universals::HttpMethod, SupabaseClient};


impl SupabaseClient {
    pub async fn rpc_call<Input, Output>(
        &self,
        rpc_name: &str,
        args: &Input,
    ) -> Result<Vec<Output>, String>
    where
        Input: Serialize + ?Sized,
        Output: DeserializeOwned,
    {
        let path = format!("/rest/v1/rpc/{}", rpc_name);

        // Serialize args to JSON inside request method
        let response = self
            .request(&path, HttpMethod::Post, Some(serde_json::to_value(args).map_err(|e| e.to_string())?), false)
            .await?;

        let arr = response
            .as_array()
            .ok_or_else(|| "Expected JSON array from RPC".to_string())?;

        let result = arr
            .iter()
            .map(|item| serde_json::from_value(item.clone()))
            .collect::<Result<Vec<Output>, _>>()
            .map_err(|e| format!("Deserialization error: {:?}", e))?;

        Ok(result)
    }
}
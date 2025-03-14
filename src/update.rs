use serde_json::json;

use crate::{generate_id, universals::HttpMethod, SupabaseClient};


impl SupabaseClient {
    /// Patch updates any fields you include in the body
    pub async fn update(&self, table_name: &str, id: &str, body: serde_json::Value) -> Result<(), String> {self.request(
            &format!("/rest/v1/{table_name}?id=eq.{id}"),
            &HttpMethod::Patch,
            Some(body),
            false
        ).await?;
        return Ok(())
    }

    /// Creates or updates depending on whether the ID has been used before in this table
    pub async fn upsert(&self, table_name: &str, mut body: serde_json::Value) -> Result<String, String> {
        let id = match body["id"].as_str() {
            Some(id) => id.to_string(),
            None => generate_id()
        };

        body["id"] = json!(id);

        self.request(
            &format!("/rest/v1/{table_name}"),
            &HttpMethod::Post,
            Some(body),
            true
        ).await?;

        Ok(id)
    }
}
use serde_json::json;

use crate::{generate_id, SupabaseClient};


impl SupabaseClient {
    /// Patch updates any fields you include in the body
    pub async fn update(&self, table_name: &str, id: &str, body: serde_json::Value) -> Result<(), String> {
        let endpoint = format!("{}/rest/v1/{}?id=eq.{}", self.url, table_name, id);
        let client = reqwest::Client::new();

        let response = match client
            .patch(&endpoint)
            .header("apikey", &self.api_key)
            .header("Authorization", format!("Bearer {}", &self.api_key))
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .await {
                Ok(response) => response,
                Err(e) => return Err(e.to_string())
            };

        if response.status().is_success() {
            return Ok(())
        } else {
            return Err(response.status().to_string())
        }
    }

    /// Creates or updates depending on whether the ID has been used before in this table
    pub async fn upsert(&self, table_name: &str, mut body: serde_json::Value) -> Result<String, String> {
        let id = match body["id"].as_str() {
            Some(id) => id.to_string(),
            None => generate_id()
        };

        let endpoint = format!("{}/rest/v1/{}", self.url, table_name);
        let client = reqwest::Client::new();

        body["id"] = json!(id);

        let response = match client
            .post(&endpoint)
            .header("apikey", &self.api_key)
            .header("Authorization", format!("Bearer {}", &self.api_key))
            .header("Content-Type", "application/json")
            .header("Prefer", "resolution=merge-duplicates")
            .body(body.to_string())
            .send()
            .await {
                Ok(response) => response,
                Err(e) => return Err(e.to_string())
            };

        if response.status().is_success() {
            return Ok(id)
        } else {
            return Err(response.status().to_string())
        }
    }
}
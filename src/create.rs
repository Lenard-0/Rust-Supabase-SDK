use serde_json::json;

use crate::{SupabaseClient, generate_id};


impl SupabaseClient {
    /// Creates a new record using UUID as the primary key (this is included inside this function)
    /// Must use UUID as primary record
    /// Returns ID as String
    pub async fn create(&self, table_name: &str, mut body: serde_json::Value) -> Result<String, String> {
        let endpoint = format!("{}/rest/v1/{}", self.url, table_name);
        let client = reqwest::Client::new();
        let new_id = generate_id();
        body["id"] = json!(new_id);

        // Make a GET request to the user endpoint
        let response = match client
            .post(&endpoint)
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
            return Ok(new_id)
        } else {
            return Err(response.status().to_string())
        }
    }
}
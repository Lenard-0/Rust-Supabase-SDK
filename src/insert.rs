use serde_json::{json, Value};

use crate::{generate_id, universals::HttpMethod, SupabaseClient};

impl SupabaseClient {
    /// Creates a new record using UUID as the primary key (this is included inside this function)
    /// Must use UUID as primary record
    /// Returns ID as String
    pub async fn insert(
        &self,
        table_name: &str,
        mut body: Value
    ) -> Result<String, String> {
        let new_id = generate_id();
        body["id"] = json!(new_id);

        self.request(
        &format!("/rest/v1/{table_name}"),
        HttpMethod::Post,
        Some(body),
            false
        ).await?;

        Ok(new_id)
    }
}

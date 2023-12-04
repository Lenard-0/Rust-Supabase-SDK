use crate::SupabaseClient;


impl SupabaseClient {
    pub async fn delete(&self, table_name: &str, id: &str) -> Result<(), String> {
        let endpoint = format!("{}/rest/v1/{}?id=eq.{}", self.url, table_name, id);
        let client = reqwest::Client::new();

        let response = match client
            .delete(&endpoint)
            .header("apikey", &self.api_key)
            .header("Authorization", format!("Bearer {}", &self.api_key))
            .header("Content-Type", "application/json")
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
}
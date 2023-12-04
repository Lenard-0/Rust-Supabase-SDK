use crate::SupabaseClient;


impl SupabaseClient {
    /// Returns the specified record as JSON
    pub async fn get_by_id(&self, table_name: &str, id: &str) -> Result<serde_json::Value, String> {
        let endpoint = format!("{}/rest/v1/{}?id=eq.{}", self.url, table_name, id);
        let client = reqwest::Client::new();

        let response = match client
            .get(&endpoint)
            .header("apikey", &self.api_key)
            .header("Authorization", format!("Bearer {}", &self.api_key))
            .header("Content-Type", "application/json")
            .send()
            .await {
                Ok(response) => response,
                Err(e) => return Err(e.to_string())
            };

        if response.status().is_success() {
            let json: serde_json::Value = response.json().await.unwrap();
            return match json.as_array() {
                Some(arr) => match arr.len() > 0 {
                    true => Ok(json[0].clone()),
                    false => return Err(format!("No record found for type: {table_name} id: {id}"))
                },
                None => return Err(format!("No record found for type: {table_name} id: {id}"))
            }
        } else {
            return Err(response.status().to_string())
        }
    }
}
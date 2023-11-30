use serde_json::json;
use uuid::Uuid;


pub struct SupabaseClient {
    pub url: String,
    pub api_key: String,
}

impl SupabaseClient {
    /// Service role and private key are synonymous
    pub fn new(supabase_url: String, private_key: String) -> Self {
        Self {
            url: supabase_url,
            api_key: private_key,
        }
    }
}

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
            let json: serde_json::Value = json.as_array().unwrap()[0].clone();
            return Ok(json)
        } else {
            return Err(response.status().to_string())
        }
    }

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

    /// Creates a new record using UUID as the primary key (this is included inside this function)
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

pub fn generate_id() -> String {
    Uuid::new_v4().to_string()
}
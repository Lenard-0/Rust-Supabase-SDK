use serde_json::Value;

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

    /// Returns all records from the specified table as JSON
    pub async fn get_all(&self, table_name: &str) -> Result<Vec<Value>, String> {
        let endpoint = format!("{}/rest/v1/{}", self.url, table_name);
        let client = reqwest::Client::new();
        let mut all_records = Vec::new();
        let limit = 100; // Set a fixed limit
        let mut offset = 0;

        loop {
            let response = match client
                .get(&endpoint)
                .header("apikey", &self.api_key)
                .header("Authorization", format!("Bearer {}", &self.api_key))
                .header("Content-Type", "application/json")
                .query(&[("limit", limit.to_string()), ("offset", offset.to_string())])
                .send()
                .await {
                    Ok(response) => response,
                    Err(e) => return Err(e.to_string())
                };

            if response.status().is_success() {
                let json: Result<Vec<Value>, reqwest::Error> = response.json().await;
                match json {
                    Ok(mut records) => {
                        if records.is_empty() {
                            break;
                        }
                        all_records.append(&mut records);
                        offset += limit;
                    },
                    Err(e) => return Err(e.to_string())
                }
            } else {
                return Err(response.status().to_string())
            }
        }

        Ok(all_records)
    }
}
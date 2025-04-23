use std::time::Duration;

use reqwest::{Client, StatusCode};
use serde_json::Value;
use crate::SupabaseClient;



pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl SupabaseClient {
    pub async fn request(
        &self,
        path: &str,
        method: HttpMethod,
        payload: Option<Value>,
        upsert: bool,
    ) -> Result<Value, String> {
        let client = Client::new();
        let max_retries = 5;

        for attempt in 0..=max_retries {
            let req_url = format!("{}{}", self.url, path);
            let mut req = match method {
                HttpMethod::Get => client.get(&req_url),
                HttpMethod::Post => client.post(&req_url),
                HttpMethod::Put => client.put(&req_url),
                HttpMethod::Patch => client.patch(&req_url),
                HttpMethod::Delete => client.delete(&req_url),
            }
            .bearer_auth(&self.api_key)
            .header("apikey", &self.api_key);

            if upsert {
                req = req.header("Prefer", "resolution=merge-duplicates");
            }

            if let Some(ref data) = payload {
                req = req.json(data);
            }

            match req.send().await {
                Ok(resp) => {
                    let status = resp.status();

                    if status == StatusCode::TOO_MANY_REQUESTS && attempt < max_retries {
                        tokio::time::sleep(Duration::from_millis(50 * 2_u64.pow(attempt))).await;
                        continue;
                    }

                    if !status.is_success() {
                        let body = resp.text().await.unwrap_or_else(|_| "".into());
                        return Err(format!(
                            "HTTP error {} on path {}\nPayload: {:#?}\nBody: {}",
                            status, path, payload, body
                        ));
                    }

                    let text = resp
                        .text()
                        .await
                        .map_err(|e| format!("Failed to read response: {:?}", e))?;

                    return if text.is_empty() {
                        Ok(Value::Null)
                    } else {
                        serde_json::from_str(&text)
                            .map_err(|e| format!("JSON parse error: {:?}", e))
                    };
                }

                Err(e) => return Err(format!("Request error: {:?}", e)),
            }
        }

        Err("Exceeded max retries".into())
    }
}
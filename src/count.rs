use reqwest::{header::HeaderValue, Client};
use url::Url;
use crate::{select::SelectQuery, SupabaseClient};


impl SupabaseClient {
    pub async fn count(&self, table_name: &str, query: SelectQuery) -> Result<usize, String> {
        let mut url = Url::parse(&format!("{}/rest/v1/{}", self.url, table_name))
            .map_err(|e| e.to_string())?;

        // Keep original query string (just filters and sorts)
        let query_string = query.to_query_string();
        url.set_query(Some(&query_string));

        let client = Client::new();
        let response = client
            .get(url)
            .header("apikey", HeaderValue::from_str(&self.api_key).map_err(|e| e.to_string())?)
            .header("Authorization", format!("Bearer {}", &self.api_key))
            .header("Prefer", "count=exact") // <-- FIX: Pass as header
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| format!("HTTP request error: {:?}", e))?;

        let res_status = response.status();
        if !res_status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Request failed with status: {}\n{}", res_status, body));
        }

        // Read Content-Range header
        match response.headers().get("content-range") {
            Some(val) => {
                let val = val.to_str().map_err(|e| e.to_string())?;
                if let Some(total_str) = val.split('/').nth(1) {
                    total_str
                        .trim()
                        .parse::<usize>()
                        .map_err(|e| format!("Failed to parse count from header: {:?}", e))
                } else {
                    Err("Malformed content-range header".to_string())
                }
            }
            None => Err("Missing content-range header in response".to_string()),
        }
    }
}
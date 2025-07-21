use reqwest::{header::HeaderValue, Client};
use url::Url;
use crate::{select::SelectQuery, SupabaseClient};


impl SupabaseClient {
    pub async fn count(&self, table_name: &str, query: SelectQuery) -> Result<usize, String> {
        let mut url = Url::parse(&format!("{}/rest/v1/{}", self.url, table_name))
            .map_err(|e| e.to_string())?;

        // Convert existing query to string and add count=exact
        let mut query_string = query.to_query_string();
        if !query_string.is_empty() {
            query_string.push('&');
        }
        query_string.push_str("count=exact");

        url.set_query(Some(&query_string));

        let client = Client::new();
        let response = client
            .get(url)
            .header("apikey", HeaderValue::from_str(&self.api_key).map_err(|e| e.to_string())?)
            .header("Authorization", format!("Bearer {}", &self.api_key))
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| format!("HTTP request error: {:?}", e))?;

        let response_status = response.status();
        if !response_status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Request failed with status: {}\n{}", response_status, body));
        }

        // Get the count from the `content-range` header
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

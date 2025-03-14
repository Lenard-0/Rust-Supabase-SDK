use reqwest::Client;
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
        method: &HttpMethod,
        payload: Option<Value>,
        upsert: bool
    ) -> Result<Value, String> {
        let mut reqwest_builder = match method {
            HttpMethod::Get => Client::new().get(self.url.clone() + path),
            HttpMethod::Post => Client::new().post(self.url.clone() + path),
            HttpMethod::Put => Client::new().put(self.url.clone() + path),
            HttpMethod::Patch => Client::new().patch(self.url.clone() + path),
            HttpMethod::Delete => Client::new().delete(self.url.clone() + path),
        };

        reqwest_builder = reqwest_builder.bearer_auth(&self.api_key);
        reqwest_builder = reqwest_builder.header("apikey", &self.api_key);

        if upsert {
            reqwest_builder = reqwest_builder.header("Prefer", "resolution=merge-duplicates");
        }

        let response = reqwest_builder
            .json(&payload)
            .send()
            .await
            .map_err(|err| format!("Error sending request: {:#?}", err))?;

        let status = response.status();
        if !status.is_success() {
            let response_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read response body".to_string());

            return Err(format!(
                "Error: received status code {}\npath: {}\npayload: {:#?}\nresponse body: {}",
                status, path, payload, response_text
            ))
        }

        let result_str = response
            .text()
            .await
            .map_err(|err| format!("Error reading response body: {:#?}", err))?;

        if result_str.is_empty() {
            return Ok(Value::Null);
        }

        return match result_str.parse::<Value>() {
            Ok(value) => Ok(value),
            Err(err) => Err(format!("Error converting response to JSON: {:#?}", err)),
        }
    }
}
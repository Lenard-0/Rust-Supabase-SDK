use reqwest::Client;
use serde_json::json;
use crate::SupabaseClient;

impl SupabaseClient {
    pub async fn forgot_password(&self, email: &str) -> Result<(), String> {
        let request_url = format!("{}/auth/v1/recover", self.url);
        let client = Client::new();
        let json = json!({
            "email": email
        });
        let response = client
            .post(request_url)
            .header("apikey", &self.api_key)
            .header("Authorization", format!("Bearer {}", &self.api_key))
            .header("Content-Type", "application/json")
            .json(&json)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            let error_message = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            Err(format!("Insert request failed with status {}: {}", status, error_message))
        }
    }

    pub async fn reset_password(&self, new_password: &str, access_token: &str, otp: &str) -> Result<(), String> {
        let request_url = format!("{}/auth/v1/user", self.url);
        let client = Client::new();
        let json = json!({
            "password": new_password,
            "code": otp
        });

        let response = client
            .put(&request_url)
            .header("apikey", &self.api_key)
            .header("Authorization", format!("Bearer {}", access_token))
            .header("Content-Type", "application/json")
            .json(&json)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        let status = response.status();
        if status.is_success() {
            println!("Reset password request successful");
            Ok(())
        } else {
            let error_message = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            eprintln!("Reset password request failed with status {}: {}", status, error_message);
            Err(format!("Reset password request failed with status {}: {}", status, error_message))
        }
    }
}
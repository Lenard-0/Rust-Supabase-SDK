use reqwest::{Client, Error};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::SupabaseClient;

#[derive(Serialize)]
pub struct SignUpRequest {
    pub email: String,
    pub password: String,
    pub user_id: Option<String>,
    pub name: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub expires_in: u64,
    pub refresh_token: String,
    pub token_type: String,
    pub user: serde_json::Value,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct AuthErrorResponse {
    pub code: u32,
    pub msg: String,
    pub error_code: String,
}

impl AuthErrorResponse {
    pub fn from_string(error: String) -> AuthErrorResponse {
        AuthErrorResponse {
            code: 0,
            msg: error.clone(),
            error_code: error
        }
    }
}

impl SupabaseClient {
    pub async fn sign_up(&self, sign_up_request: SignUpRequest) -> Result<AuthResponse, AuthErrorResponse> {
        let client = Client::new();
        let url = format!("{}/auth/v1/signup", self.url);

        let response = match client
            .post(&url)
            .header("apikey", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&sign_up_request)
            .send()
            .await {
            Ok(response) => response,
            Err(err) => return Err(AuthErrorResponse::from_string(format!("Failed to send sign up request: {:?}", err).into())),
        };

        let auth_response_json: Value = match response.json().await {
            Ok(json) => json,
            Err(err) => return Err(AuthErrorResponse::from_string(format!("Failed to parse sign up response: {:?}", err).into())),
        };
        println!("Auth response: {:#?}", auth_response_json);
        match auth_response_json["error_code"].as_str() {
            Some(_) => {
                let auth_error_response: AuthErrorResponse = match serde_json::from_value(auth_response_json) {
                    Ok(auth_error_response) => auth_error_response,
                    Err(err) => return Err(AuthErrorResponse::from_string(format!("Failed to parse sign up error response: {:?}", err).into())),
                };
                return Err(auth_error_response)
            }
            _ => {}
        };
        let auth_response: AuthResponse = match serde_json::from_value(auth_response_json) {
            Ok(auth_response) => auth_response,
            Err(err) => return Err(AuthErrorResponse::from_string(format!("Failed to parse sign up response: {:?}", err).into())),
        };
        Ok(auth_response)
    }

    pub async fn sign_in(&self, email: &str, password: &str) -> Result<AuthResponse, Error> {
        let client = Client::new();
        let url = format!("{}/auth/v1/token?grant_type=password", self.url);
        let request_body = json!( {
            "email": email,
            "password": password,
        });

        let response = client
            .post(&url)
            .header("apikey", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        let auth_response = response.json::<AuthResponse>().await?;
        Ok(auth_response)
    }

    pub async fn get_user(&self, access_token: &str) -> Result<serde_json::Value, Error> {
        let client = Client::new();
        let url = format!("{}/auth/v1/user", self.url);

        let response = client
            .get(&url)
            .header("apikey", &self.api_key)
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await?;

        let user_info = response.json::<serde_json::Value>().await?;
        Ok(user_info)
    }

    pub async fn delete_user(&self, user_id: &str) -> Result<(), String> {
        let client = Client::new();
        let url = format!("{}/auth/v1/admin/users/{}", self.url, user_id);

        let response = match client
            .delete(&url)
            .header("apikey", &self.api_key)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await {
            Ok(response) => response,
            Err(err) => return Err(format!("Failed to send request: {:?}", err).into()),
        };

        if response.status().is_success() {
            println!("User {} deleted successfully", user_id);
            Ok(())
        } else {
            Err(format!("Failed to delete user: {:?}", match response.text().await {
                Ok(text) => text,
                Err(err) => format!("Failed to read response body: {:?}", err),
            }).into())
        }
    }
}
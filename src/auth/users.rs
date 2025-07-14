use reqwest::Client;
use serde::Deserialize;

use crate::SupabaseClient;


#[derive(Debug, Deserialize)]
pub struct SupabaseUser {
    pub id: String,
    pub email: Option<String>,
    pub created_at: i64,
}

impl SupabaseClient {
    pub async fn get_all_users(&self) -> Result<Vec<SupabaseUser>, reqwest::Error> {
        let client = Client::new();
        let url = format!("{}/auth/v1/admin/users", self.url);

        let response = client
            .get(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        let users: Vec<SupabaseUser> = serde_json::from_value(response["users"].clone()).unwrap_or_default();
        Ok(users)
    }

    pub async fn get_user_by_id(&self, user_id: &str) -> Result<SupabaseUser, reqwest::Error> {
        let client = Client::new();
        let url = format!("{}/auth/v1/admin/users/{}", self.url, user_id);

        let response = client
            .get(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await?
            .json::<SupabaseUser>()
            .await?;

        Ok(response)
    }
}


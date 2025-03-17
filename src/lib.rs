use uuid::Uuid;

pub mod get;
pub mod delete;
pub mod update;
pub mod insert;
pub mod select;
pub mod universals;
pub mod auth;

#[derive(Debug, Clone)]
pub struct SupabaseClient {
    pub url: String,
    pub api_key: String,
    pub access_token: Option<String>,
}

impl SupabaseClient {
    /// Service role and private key are synonymous
    pub fn new(supabase_url: String, private_key: String, access_token: Option<String>) -> Self {
        Self {
            url: supabase_url,
            api_key: private_key,
            access_token, // Initialize access token
        }
    }
}

pub fn generate_id() -> String {
    Uuid::new_v4().to_string()
}

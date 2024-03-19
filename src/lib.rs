
use uuid::Uuid;

pub mod get;
pub mod delete;
pub mod update;
pub mod create;
pub mod select;

#[derive(Debug, Clone)]
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

pub fn generate_id() -> String {
    Uuid::new_v4().to_string()
}
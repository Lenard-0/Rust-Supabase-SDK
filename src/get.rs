use crate::{universals::HttpMethod, SupabaseClient};


impl SupabaseClient {
    /// Returns the specified record as JSON
    pub async fn get_by_id(&self, table_name: &str, id: &str) -> Result<serde_json::Value, String> {
        let result = self.request(
            &format!("/rest/v1/{table_name}?id=eq.{id}"),
            &HttpMethod::Get,
            None,
            false
        ).await?;

        return match result.as_array() {
            Some(arr) => match arr.len() > 0 {
                true => Ok(arr[0].clone()),
                false => return Err(format!("No record found for type: {table_name} id: {id}"))
            },
            None => return Err(format!("No record found for type: {table_name} id: {id}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use dotenv::dotenv;
    use serde_json::json;
    use rust_supabase_sdk::{SupabaseClient, generate_id};

    #[tokio::test]
    async fn can_initialise_supabase_client() {
        dotenv().ok(); // Load environment variables from .env file
        SupabaseClient::new(
            std::env::var("SUPABASE_URL").unwrap(),
            std::env::var("SUPABASE_KEY").unwrap(),
        );
    }

    #[tokio::test]
    async fn can_update_and_get() {
        dotenv().ok(); // Load environment variables from .env file

        let supabase_client = SupabaseClient::new(
            std::env::var("SUPABASE_URL").unwrap(),
            std::env::var("SUPABASE_KEY").unwrap(),
        );
        // generate random number to test update has worked
        let random_number = format!("{}", rand::random::<u32>());
        let body = json!(
            {
                "access_token": random_number,
            }
        );
        supabase_client.update("access_token", "b488536a-8935-4aa7-b283-8fa1289b2c15", body).await.unwrap();
        let json_token = supabase_client.get_by_id("access_token", "b488536a-8935-4aa7-b283-8fa1289b2c15").await.unwrap();
        assert_eq!(json_token["access_token"], random_number);
    }

    #[tokio::test]
    async fn can_create() {
        dotenv().ok(); // Load environment variables from .env file
        let supabase_client = SupabaseClient::new(
            std::env::var("SUPABASE_URL").unwrap(),
            std::env::var("SUPABASE_KEY").unwrap(),
        );
        println!("{:?}", generate_id().len());
        let id = supabase_client.create("access_token", json!(
            {
                "access_token": "test",
                "refresh_token": "test",
                "expires_in": Utc::now().to_string()
            }
        )).await.unwrap();
        let json_token = supabase_client.get_by_id("access_token", &id).await.unwrap();
        assert_eq!(json_token["access_token"], "test");
        assert_eq!(json_token["refresh_token"], "test");
    }
}
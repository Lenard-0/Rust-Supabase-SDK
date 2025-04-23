
#[cfg(test)]
mod tests {
    use dotenv::dotenv;
    use serde_json::json;
    use rust_supabase_sdk::SupabaseClient;

    #[tokio::test]
    async fn can_initialise_supabase_client() {
        dotenv().ok(); // Load environment variables from .env file
        SupabaseClient::new(
            std::env::var("SUPABASE_URL").unwrap(),
            std::env::var("SUPABASE_API_KEY").unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn can_create_get_update_and_remove() {
        dotenv().ok(); // Load environment variables from .env file
        let supabase_client = SupabaseClient::new(
            std::env::var("SUPABASE_URL").unwrap(),
            std::env::var("SUPABASE_API_KEY").unwrap(),
            None
        );

        let record_id = supabase_client.insert("organisations", json!({
            "name": "Test Organisation"
        })).await.unwrap();

        let record = supabase_client.get_by_id("organisations", &record_id).await.unwrap();
        assert_eq!(record["name"], "Test Organisation");

        supabase_client.update("organisations", &record_id, json!({
            "name": "Updated Organisation"
        })).await.unwrap();

        let record = supabase_client.get_by_id("organisations", &record_id).await.unwrap();
        assert_eq!(record["name"], "Updated Organisation");

        supabase_client.delete("organisations", &record_id).await.unwrap();

        let record = supabase_client.get_by_id("organisations", &record_id).await;
        assert!(record.is_err());
    }

    #[tokio::test]
    async fn can_upsert() {
        dotenv().ok(); // Load environment variables from .env file
        let supabase_client = SupabaseClient::new(
            std::env::var("SUPABASE_URL").unwrap(),
            std::env::var("SUPABASE_API_KEY").unwrap(),
            None
        );

        let record_id = supabase_client.upsert("organisations", json!({
            "name": "Test Organisation"
        })).await.unwrap();

        let record = supabase_client.get_by_id("organisations", &record_id).await.unwrap();
        assert_eq!(record["name"], "Test Organisation");

        supabase_client.upsert("organisations", json!({
            "id": record_id,
            "name": "Updated Organisation"
        })).await.unwrap();

        let record = supabase_client.get_by_id("organisations", &record_id).await.unwrap();
        assert_eq!(record["name"], "Updated Organisation");

        supabase_client.delete("organisations", &record_id).await.unwrap();
    }
}
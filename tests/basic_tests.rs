
#[cfg(test)]
mod tests {
    use std::{thread::sleep, time::Duration};

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

    // #[tokio::test]
    // async fn can_select() {
    //     dotenv().ok(); // Load environment variables from .env file
    //     let supabase_client = SupabaseClient::new(
    //         std::env::var("SUPABASE_URL").unwrap(),
    //         std::env::var("SUPABASE_KEY").unwrap(),
    //         None
    //     );
    //     // for _ in 0..200 {
    //     //     supabase_client.create("testing", json!({})).await.unwrap();
    //     //     sleep(Duration::from_millis(30));
    //     // }

    //     let table_name = "contacts";
    //     let query = "email=eq.mlaughlin@allen-vellone.com";


    //     let json = supabase_client.select(table_name, query).await.unwrap();
    //     assert_eq!(json.len() > 1, true);

    //     // delete all items created for test
    //     for item in json {
    //         supabase_client.delete("testing", item["id"].as_str().unwrap()).await.unwrap();
    //         sleep(Duration::from_millis(30));
    //     }
    // }
}
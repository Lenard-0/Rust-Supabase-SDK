
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
    async fn can_update_and_get() {
        dotenv().ok(); // Load environment variables from .env file

        let supabase_client = SupabaseClient::new(
            std::env::var("SUPABASE_URL").unwrap(),
            std::env::var("SUPABASE_KEY").unwrap(),
            None
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
            None
        );
        println!("{:?}", generate_id().len());
        let id = supabase_client.insert("access_token", json!(
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

    #[tokio::test]
    async fn can_upsert_and_delete() {
        dotenv().ok(); // Load environment variables from .env file
        let supabase_client = SupabaseClient::new(
            std::env::var("SUPABASE_URL").unwrap(),
            std::env::var("SUPABASE_KEY").unwrap(),
            None
        );

        let id = generate_id();

        supabase_client.upsert("access_token", &id, json!(
            {
                "access_token": "upsert_test",
                "refresh_token": "upsert_test",
                "expires_in": Utc::now().to_string()
            }
        )).await.unwrap();

        let json_token = supabase_client.get_by_id("access_token", &id).await.unwrap();
        assert_eq!(json_token["access_token"], "upsert_test");
        assert_eq!(json_token["refresh_token"], "upsert_test");

        supabase_client.upsert("access_token", &id, json!(
            {
                "access_token": "test_update",
                "refresh_token": "test_update",
                "expires_in": Utc::now().to_string()
            }
        )).await.unwrap();

        let json_token = supabase_client.get_by_id("access_token", &id).await.unwrap();
        assert_eq!(json_token["access_token"], "test_update");
        assert_eq!(json_token["refresh_token"], "test_update");

        supabase_client.delete("access_token", &id).await.unwrap();

        let json_token = supabase_client.get_by_id("access_token", &id).await;
        assert!(json_token.is_err());
    }

    #[tokio::test]
    async fn can_get_all() {
        dotenv().ok(); // Load environment variables from .env file
        let supabase_client = SupabaseClient::new(
            std::env::var("SUPABASE_URL").unwrap(),
            std::env::var("SUPABASE_KEY").unwrap(),
            None
        );
        for _ in 0..200 {
            supabase_client.insert("testing", json!({})).await.unwrap();
            sleep(Duration::from_millis(30));
        }

        let json = supabase_client.get_all("testing").await.unwrap();
        assert_eq!(json.len(), 200);

        // delete all items created for test
        for item in json {
            supabase_client.delete("testing", item["id"].as_str().unwrap()).await.unwrap();
            sleep(Duration::from_millis(30));
        }
    }

    #[tokio::test]
    async fn can_select() {
        dotenv().ok(); // Load environment variables from .env file
        let supabase_client = SupabaseClient::new(
            std::env::var("SUPABASE_URL").unwrap(),
            std::env::var("SUPABASE_KEY").unwrap(),
            None
        );
        // for _ in 0..200 {
        //     supabase_client.create("testing", json!({})).await.unwrap();
        //     sleep(Duration::from_millis(30));
        // }

        let table_name = "contacts";
        let query = "email=eq.mlaughlin@allen-vellone.com";


        let json = supabase_client.select(table_name, query).await.unwrap();
        assert_eq!(json.len() > 1, true);

        // delete all items created for test
        for item in json {
            supabase_client.delete("testing", item["id"].as_str().unwrap()).await.unwrap();
            sleep(Duration::from_millis(30));
        }
    }
}
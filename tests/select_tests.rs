#[cfg(test)]
mod tests {
    use dotenv::dotenv;
    use rust_supabase_sdk::{
        select::{Filter, FilterGroup, LogicalOperator, Operator, SelectQuery, Sort, SortDirection},
        SupabaseClient
    };
    use serde_json::{json, Value};
    use std::env;
    use tokio::time::{sleep, Duration};
    use rust_supabase_sdk::query;

    /// Utility cleanup function to delete records from the given table.
    async fn cleanup_records(client: &SupabaseClient, table_name: &str, records: &[Value]) {
        for record in records {
            if let Some(id) = record["id"].as_str() {
                let _ = client.delete(table_name, id).await;
                sleep(Duration::from_millis(30)).await;
            }
        }
    }

    #[tokio::test]
    async fn clean_up() {
        dotenv().ok();
        let supabase_client = SupabaseClient::new(
            env::var("SUPABASE_URL").unwrap(),
            env::var("SUPABASE_API_KEY").unwrap(),
            None,
        );
        let table_name = "test_data";
        let records = supabase_client.select(table_name, SelectQuery::new()).await.unwrap();
        cleanup_records(&supabase_client, table_name, &records).await;
    }

    #[tokio::test]
    async fn test_eq_query() {
        dotenv().ok();
        let supabase_client = SupabaseClient::new(
            env::var("SUPABASE_URL").unwrap(),
            env::var("SUPABASE_API_KEY").unwrap(),
            None,
        );
        let table_name = "test_data";

        // Insert 30 records with name "Test Organisation"
        for _ in 0..30 {
            supabase_client.insert(table_name, json!({ "name": "Test Organisation" })).await.unwrap();
            sleep(Duration::from_millis(30)).await;
        }
        // Insert one record with a different name.
        let diff_id = supabase_client.insert(table_name, json!({ "name": "Different Organisation" })).await.unwrap();

        // Build a DSL query using our operators via the q! macro.
        let filter = query!("name" == "Test Organisation").to_filter_group();
        let select_query = SelectQuery { filter: Some(filter), sorts: Vec::new() };

        let records = supabase_client.select(table_name, select_query).await.unwrap();
        assert_eq!(records.len(), 30);

        clean_up();
        let _ = supabase_client.delete(table_name, &diff_id).await;
    }

    #[tokio::test]
    async fn test_and_query() {
        dotenv().ok();
        let supabase_client = SupabaseClient::new(
            env::var("SUPABASE_URL").unwrap(),
            env::var("SUPABASE_API_KEY").unwrap(),
            None,
        );
        let table_name = "test_data";

        let _ = supabase_client.insert(table_name, json!({ "name": "Org A", "category": "Finance" })).await.unwrap();
        let _ = supabase_client.insert(table_name, json!({ "name": "Org A", "category": "Tech" })).await.unwrap();

        // Use DSL operators combined with & for an AND query.
        let filter = (query!("name" == "Org A") & query!("category" == "Tech")).to_filter_group();
        let select_query = SelectQuery { filter: Some(filter), sorts: Vec::new() };

        let records = supabase_client.select(table_name, select_query).await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["category"], "Tech");

        clean_up();
    }

    #[tokio::test]
    async fn test_or_query() {
        dotenv().ok();
        let supabase_client = SupabaseClient::new(
            env::var("SUPABASE_URL").unwrap(),
            env::var("SUPABASE_API_KEY").unwrap(),
            None,
        );
        let table_name = "test_data";

        let id_x = supabase_client.insert(table_name, json!({ "name": "Org X" })).await.unwrap();
        let id_z = supabase_client.insert(table_name, json!({ "name": "Org Z" })).await.unwrap();

        // Use DSL operators combined with | for an OR query.
        let filter = (query!("name" == "Org X") | query!("name" == "Org Z")).to_filter_group();
        let select_query = SelectQuery { filter: Some(filter), sorts: Vec::new() };

        let records = supabase_client.select(table_name, select_query).await.unwrap();
        assert_eq!(records.len(), 2);

        // check IDs
        let ids: Vec<&str> = records.iter().map(|r| r["id"].as_str().unwrap()).collect();
        assert!(ids.contains(&id_x.as_str()));
        assert!(ids.contains(&id_z.as_str()));

        cleanup_records(&supabase_client, table_name, &records).await;
    }

    #[tokio::test]
    async fn test_sorting_created_at() {
        dotenv().ok();
        let supabase_client = SupabaseClient::new(
            env::var("SUPABASE_URL").unwrap(),
            env::var("SUPABASE_API_KEY").unwrap(),
            None,
        );
        let table_name = "test_data";

        for i in 1..=5 {
            supabase_client.insert(table_name, json!({ "name": format!("Org {}", i) })).await.unwrap();
            sleep(Duration::from_millis(30)).await;
        }

        let asc_sort = Sort::new("created_at", SortDirection::Asc);
        let desc_sort = Sort::new("created_at", SortDirection::Desc);

        let asc_query = SelectQuery { filter: None, sorts: vec![asc_sort] };
        let desc_query = SelectQuery { filter: None, sorts: vec![desc_sort] };

        let asc_records = supabase_client.select(table_name, asc_query).await.unwrap();
        let desc_records = supabase_client.select(table_name, desc_query).await.unwrap();

        let asc_dates: Vec<&str> = asc_records.iter().map(|r| r["created_at"].as_str().unwrap()).collect();
        let desc_dates: Vec<&str> = desc_records.iter().map(|r| r["created_at"].as_str().unwrap()).collect();

        assert_eq!(asc_dates.iter().rev().cloned().collect::<Vec<&str>>(), desc_dates);

        cleanup_records(&supabase_client, table_name, &asc_records).await;
    }

    /// Test the not equal operator (using !=).
    #[tokio::test]
    async fn test_neq_operator() {
        dotenv().ok();
        let client = SupabaseClient::new(
            env::var("SUPABASE_URL").unwrap(),
            env::var("SUPABASE_API_KEY").unwrap(),
            None,
        );
        let table = "test_data";

        // Insert three records with different names.
        let id1 = client.insert(table, json!({ "name": "Alice" })).await.unwrap();
        let id2 = client.insert(table, json!({ "name": "Bob" })).await.unwrap();
        let id3 = client.insert(table, json!({ "name": "Charlie" })).await.unwrap();
        sleep(Duration::from_millis(30)).await;

        // Build a DSL query: select records where name != "Alice"
        let filter = query!("name" != "Alice").to_filter_group();
        let select_query = SelectQuery { filter: Some(filter), sorts: Vec::new() };
        let records = client.select(table, select_query).await.unwrap();

        // Expect Bob and Charlie (2 records).
        assert_eq!(records.len(), 2);
        for rec in &records {
            assert_ne!(rec["name"].as_str().unwrap(), "Alice");
        }
        cleanup_records(&client, table, &records).await;
        cleanup_records(&client, table, &vec![json!({ "id": id2, "id": id3 })]).await;
        let _ = client.delete(table, &id1).await;
    }

    /// Test the greater than operator (using >).
    #[tokio::test]
    async fn test_gt_operator() {
        dotenv().ok();
        let client = SupabaseClient::new(
            env::var("SUPABASE_URL").unwrap(),
            env::var("SUPABASE_API_KEY").unwrap(),
            None,
        );
        let table = "test_data";

        // Insert records with a numeric "score" field.
        let id1 = client.insert(table, json!({ "name": "Item1", "score": 50 })).await.unwrap();
        let id2 = client.insert(table, json!({ "name": "Item2", "score": 75 })).await.unwrap();
        let id3 = client.insert(table, json!({ "name": "Item3", "score": 100 })).await.unwrap();
        sleep(Duration::from_millis(30)).await;

        // Build a DSL query: select records where score > 60.
        let filter = query!("score" > 60).to_filter_group();
        let select_query = SelectQuery { filter: Some(filter), sorts: Vec::new() };
        let records = client.select(table, select_query).await.unwrap();

        // Expect records with scores 75 and 100.
        assert_eq!(records.len(), 2);
        for rec in &records {
            let score = rec["score"].as_i64().unwrap();
            assert!(score > 60);
        }
        cleanup_records(&client, table, &records).await;
        cleanup_records(&client, table, &vec![
            json!({ "id": id2 }),
            json!({ "id": id3 }),
            ]).await;
        let _ = client.delete(table, &id1).await;
    }

    /// Test the less than operator (using <).
    #[tokio::test]
    async fn test_lt_operator() {
        dotenv().ok();
        let client = SupabaseClient::new(
            env::var("SUPABASE_URL").unwrap(),
            env::var("SUPABASE_API_KEY").unwrap(),
            None,
        );
        let table = "test_data";

        // Insert records with a numeric "score" field.
        let id1 = client.insert(table, json!({ "name": "Item1", "score": 10 })).await.unwrap();
        let id2 = client.insert(table, json!({ "name": "Item2", "score": 20 })).await.unwrap();
        let id3 = client.insert(table, json!({ "name": "Item3", "score": 30 })).await.unwrap();
        sleep(Duration::from_millis(30)).await;

        // Build a DSL query: select records where score < 25.
        let filter = query!("score" < 25).to_filter_group();
        let select_query = SelectQuery { filter: Some(filter), sorts: Vec::new() };
        let records = client.select(table, select_query).await.unwrap();

        // Expect items with score 10 and 20.
        assert_eq!(records.len(), 2);
        for rec in &records {
            let score = rec["score"].as_i64().unwrap();
            assert!(score < 25);
        }
        cleanup_records(&client, table, &records).await;
        cleanup_records(&client, table, &vec![
            json!({ "id": id1 }),
            json!({ "id": id2 }),
            ]).await;
        let _ = client.delete(table, &id3).await;
    }

    /// Test the like operator.
    ///
    /// The LIKE operator in PostgREST supports SQL-like pattern matching.
    /// For example, the pattern "Alph%" will match any string starting with "Alph" (the "%" is a wildcard).
    #[tokio::test]
    async fn test_like_operator() {
        dotenv().ok();
        let client = SupabaseClient::new(
            env::var("SUPABASE_URL").unwrap(),
            env::var("SUPABASE_API_KEY").unwrap(),
            None,
        );
        let table = "test_data";

        // Insert records with names that follow a pattern.
        let id1 = client.insert(table, json!({ "name": "Alpha" })).await.unwrap();
        let id2 = client.insert(table, json!({ "name": "Beta" })).await.unwrap();
        let id3 = client.insert(table, json!({ "name": "Alphabet" })).await.unwrap();
        sleep(Duration::from_millis(30)).await;

        // Build a DSL query: select records where name is like "Alph%"
        // The pattern "Alph%" will match "Alpha" and "Alphabet" (since "%" is a wildcard).
        let filter = FilterGroup::new(
            LogicalOperator::Or,
            vec![
                Filter {
                    column: "name".to_string(),
                    operator: Operator::Like,
                    value: "Alph%".to_string(),
                }
            ],
        );
        let select_query = SelectQuery { filter: Some(filter), sorts: Vec::new() };
        let records = client.select(table, select_query).await.unwrap();

        // Expect to match 2 records: "Alpha" and "Alphabet".
        assert_eq!(records.len(), 2);
        for rec in &records {
            let name = rec["name"].as_str().unwrap();
            assert!(name.starts_with("Alph"));
        }
        cleanup_records(&client, table, &records).await;
        cleanup_records(&client, &table, &vec![
            json!({ "id": id1 }),
            json!({ "id": id2 }),
            json!({ "id": id3 }),
            ]).await;
        let _ = client.delete(table, &id2).await;
    }
}

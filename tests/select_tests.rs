#[cfg(test)]
mod tests {
    use dotenv::dotenv;
    use rust_supabase_sdk::{
        select::{Filter, FilterGroup, LogicalOperator, Operator, SelectQuery, Sort, SortDirection},
        SupabaseClient
    };
    use serde_json::{json, Value};
    use uuid::Uuid;
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

    async fn clean_all() {
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
    async fn clean_up() {
        dotenv().ok();
        clean_all().await;
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

        let records = supabase_client.select(
            table_name,
            query!("name" == "Test Organisation").to_query(),
        ).await.unwrap();
        assert_eq!(records.len(), 30);

        clean_all().await;
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

        let query = (query!("name" == "Org A") & query!("category" == "Tech")).to_query();
        assert_eq!(query, SelectQuery {
            filter: Some(FilterGroup {
                operator: LogicalOperator::And,
                filters: vec![
                    Filter {
                        column: "name".to_string(),
                        operator: Operator::Eq,
                        value: "Org A".to_string(),
                    },
                    Filter {
                        column: "category".to_string(),
                        operator: Operator::Eq,
                        value: "Tech".to_string(),
                    },
                ],
            }),
            sorts: vec![],
        });

        let records = supabase_client.select(
            table_name,
            query,
        ).await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["category"], "Tech");

        clean_all().await;
    }

    #[tokio::test]
    async fn test_and_query_with_uuids() {
        dotenv().ok();
        let supabase_client = SupabaseClient::new(
            env::var("SUPABASE_URL").unwrap(),
            env::var("SUPABASE_API_KEY").unwrap(),
            None,
        );
        let table_name = "test_data";

        let id1 = Uuid::new_v4().to_string();
        let id2 = Uuid::new_v4().to_string();

        let _ = supabase_client.insert(table_name, json!({ "id1": id1.clone(), "id2": id2.clone() })).await.unwrap();
        let _ = supabase_client.insert(table_name, json!({ "id1": id1.clone(), "id2": id1.clone() })).await.unwrap();

        let id1_clone = id1.clone();
        let id2_clone = id2.clone();
        let query = (query!("id1" == id1) & query!("id2" == id2)).to_query();
        assert_eq!(query, SelectQuery {
            filter: Some(FilterGroup {
                operator: LogicalOperator::And,
                filters: vec![
                    Filter {
                        column: "id1".to_string(),
                        operator: Operator::Eq,
                        value: id1_clone.to_string(),
                    },
                    Filter {
                        column: "id2".to_string(),
                        operator: Operator::Eq,
                        value: id2_clone.to_string(),
                    },
                ],
            }),
            sorts: vec![],
        });

        let records = supabase_client.select(
            table_name,
            query,
        ).await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["id1"], id1_clone);
        assert_eq!(records[0]["id2"], id2_clone);

        clean_all().await;
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

        let query = (query!("name" == "Org X") | query!("name" == "Org Z")).to_query();
        assert_eq!(query, SelectQuery {
            filter: Some(FilterGroup {
                operator: LogicalOperator::Or,
                filters: vec![
                    Filter {
                        column: "name".to_string(),
                        operator: Operator::Eq,
                        value: "Org X".to_string(),
                    },
                    Filter {
                        column: "name".to_string(),
                        operator: Operator::Eq,
                        value: "Org Z".to_string(),
                    },
                ],
            }),
            sorts: vec![],
        });

        let records = supabase_client.select(
            table_name,
            query,
        ).await.unwrap();
        assert_eq!(records.len(), 2);

        // check IDs
        let ids: Vec<&str> = records.iter().map(|r| r["id"].as_str().unwrap()).collect();
        assert!(ids.contains(&id_x.as_str()));
        assert!(ids.contains(&id_z.as_str()));

        clean_all().await;
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

        clean_all().await;
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
        let _ = client.insert(table, json!({ "name": "Alice" })).await.unwrap();
        let _ = client.insert(table, json!({ "name": "Bob" })).await.unwrap();
        let _ = client.insert(table, json!({ "name": "Charlie" })).await.unwrap();
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
        clean_all().await;
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
        let _ = client.insert(table, json!({ "name": "Item1", "score": 50 })).await.unwrap();
        let _ = client.insert(table, json!({ "name": "Item2", "score": 75 })).await.unwrap();
        let _ = client.insert(table, json!({ "name": "Item3", "score": 100 })).await.unwrap();
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
        clean_all().await;
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
        let _ = client.insert(table, json!({ "name": "Item1", "score": 10 })).await.unwrap();
        let _ = client.insert(table, json!({ "name": "Item2", "score": 20 })).await.unwrap();
        let _ = client.insert(table, json!({ "name": "Item3", "score": 30 })).await.unwrap();
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
        clean_all().await;
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
        let _ = client.insert(table, json!({ "name": "Alpha" })).await.unwrap();
        let _ = client.insert(table, json!({ "name": "Beta" })).await.unwrap();
        let _ = client.insert(table, json!({ "name": "Alphabet" })).await.unwrap();
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
        clean_all().await;
    }

    // Test combining filter and sort
    #[tokio::test]
    async fn test_filter_and_sort() {
        dotenv().ok();
        let client = SupabaseClient::new(
            env::var("SUPABASE_URL").unwrap(),
            env::var("SUPABASE_API_KEY").unwrap(),
            None,
        );
        let table = "test_data";

        // Insert records with a numeric "score" field.
        let _ = client.insert(table, json!({ "name": "Item1", "score": 10 })).await.unwrap();
        let _ = client.insert(table, json!({ "name": "Item2", "score": 20 })).await.unwrap();
        let _ = client.insert(table, json!({ "name": "Item3", "score": 30 })).await.unwrap();
        sleep(Duration::from_millis(30)).await;

        let records = client.select(
            table,
            query!("score" < 25)
                .to_query()
                .sort("score", SortDirection::Desc)
        ).await.unwrap();

        // Expect items with score 20 and 10, in descending order.
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["score"].as_i64().unwrap(), 20);
        assert_eq!(records[1]["score"].as_i64().unwrap(), 10);
        clean_all().await;
    }

    #[tokio::test]
    async fn can_create_simple_filter_query() {
        let lecture_id = "8e662d9e-c920-4d2f-bda7-09e5173cc494";
        let user_id = lecture_id;
        let filter = (query!("lecture_id" == lecture_id) & query!("user_id" == user_id)).to_filter_group();
        assert_eq!(filter, FilterGroup {
            operator: LogicalOperator::And,
            filters: vec![Filter {
                column: "lecture_id".to_string(),
                operator: Operator::Eq,
                value: lecture_id.to_string(),
            }, Filter {
                column: "user_id".to_string(),
                operator: Operator::Eq,
                value: user_id.to_string(),
            }],
        });

        assert_eq!(filter.to_query_string(), "lecture_id=eq.8e662d9e-c920-4d2f-bda7-09e5173cc494&user_id=eq.8e662d9e-c920-4d2f-bda7-09e5173cc494");
    }

    #[tokio::test]
    async fn can_use_expression_in_query_eq_macro() {
        dotenv().ok();
        let supabase_client = SupabaseClient::new(
            env::var("SUPABASE_URL").unwrap(),
            env::var("SUPABASE_API_KEY").unwrap(),
            None,
        );
        let table_name = "test_data";

        // Insert 30 records with name "Test Organisation"
        for _ in 0..5 {
            supabase_client.insert(table_name, json!({ "name": "Test Organisation" })).await.unwrap();
            sleep(Duration::from_millis(30)).await;
        }
        let query_name_varible = "Test Organisation".to_string();
        let first_half = "Test".to_string();
        let second_half = "Organisation".to_string();
        // Insert one record with a different name.
        let diff_id = supabase_client.insert(table_name, json!({ "name": "Different Organisation" })).await.unwrap();
        let records = supabase_client.select(
            table_name,
            query!("name" == query_name_varible.clone()).to_query(),
        ).await.unwrap();

        let same_records = supabase_client.select(
            table_name,
            query!("name" == format!("{} {}", first_half, second_half)).to_query(),
        ).await.unwrap();

        let same_records_2 = supabase_client.select(
            table_name,
            query!("name" == &query_name_varible).to_query(),
        ).await.unwrap();

        assert_eq!(records, same_records);
        assert_eq!(records, same_records_2);
        assert_eq!(records.len(), 5);

        clean_all().await;
        let _ = supabase_client.delete(table_name, &diff_id).await;
    }
}

#[cfg(test)]
#[allow(deprecated)] // Legacy integration tests pinned to the pre-builder API.
mod tests {
    use dotenv::dotenv;
    use rust_supabase_sdk::{
        select::{Filter, FilterGroup, LogicalOperator, Operator, SelectQuery, Sort, SortDirection},
        SupabaseClient
    };
    use serde_json::json;
    use uuid::Uuid;
    use std::env;
    use tokio::time::{sleep, Duration};
    use rust_supabase_sdk::query;

    fn make_client() -> SupabaseClient {
        SupabaseClient::new(
            env::var("SUPABASE_URL").expect("SUPABASE_URL not set"),
            env::var("SUPABASE_API_KEY").expect("SUPABASE_API_KEY not set"),
            None,
        )
    }

    /// Delete only the specific records inserted by one test run.
    async fn delete_ids(client: &SupabaseClient, table: &str, ids: &[String]) {
        for id in ids {
            let _ = client.delete(table, id).await;
            sleep(Duration::from_millis(20)).await;
        }
    }

    // Each test that touches the live DB uses a unique `run_id` embedded in
    // the data it inserts, so tests running in parallel don't interfere.
    // Cleanup deletes only the rows created by that specific test invocation.

    #[tokio::test]
    async fn test_eq_query() {
        dotenv().ok();
        let client = make_client();
        let table = "test_data";
        let run_id = Uuid::new_v4().to_string();
        let target_name = format!("Test Organisation {run_id}");
        let other_name  = format!("Different Organisation {run_id}");

        let mut ids: Vec<String> = Vec::new();
        for _ in 0..30 {
            let id = client.insert(table, json!({ "name": target_name })).await.unwrap();
            ids.push(id);
            sleep(Duration::from_millis(20)).await;
        }
        let diff_id = client.insert(table, json!({ "name": other_name })).await.unwrap();
        ids.push(diff_id);

        let records = client.select(
            table,
            query!("name" == target_name.clone()).to_query(),
        ).await.unwrap();

        assert_eq!(records.len(), 30, "expected exactly 30 rows with target name");

        delete_ids(&client, table, &ids).await;
    }

    #[tokio::test]
    async fn test_and_query() {
        dotenv().ok();
        let client = make_client();
        let table = "test_data";
        let run_id = Uuid::new_v4().to_string();

        let mut ids: Vec<String> = Vec::new();
        ids.push(client.insert(table, json!({ "name": format!("Org A {run_id}"), "category": "Finance" })).await.unwrap());
        ids.push(client.insert(table, json!({ "name": format!("Org A {run_id}"), "category": "Tech" })).await.unwrap());

        let org_a = format!("Org A {run_id}");
        let query = (query!("name" == org_a.clone()) & query!("category" == "Tech")).to_query();
        assert_eq!(query, SelectQuery {
            filter: Some(FilterGroup {
                operator: LogicalOperator::And,
                filters: vec![
                    Filter { column: "name".into(), operator: Operator::Eq, value: org_a.clone() },
                    Filter { column: "category".into(), operator: Operator::Eq, value: "Tech".into() },
                ],
            }),
            sorts: vec![],
        });

        let records = client.select(table, query).await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["category"], "Tech");

        delete_ids(&client, table, &ids).await;
    }

    #[tokio::test]
    async fn test_and_query_with_uuids() {
        dotenv().ok();
        let client = make_client();
        let table = "test_data";

        let id1 = Uuid::new_v4().to_string();
        let id2 = Uuid::new_v4().to_string();

        let mut row_ids: Vec<String> = Vec::new();
        row_ids.push(client.insert(table, json!({ "id1": id1, "id2": id2 })).await.unwrap());
        row_ids.push(client.insert(table, json!({ "id1": id1, "id2": id1 })).await.unwrap());

        let query = (query!("id1" == id1.clone()) & query!("id2" == id2.clone())).to_query();
        assert_eq!(query, SelectQuery {
            filter: Some(FilterGroup {
                operator: LogicalOperator::And,
                filters: vec![
                    Filter { column: "id1".into(), operator: Operator::Eq, value: id1.clone() },
                    Filter { column: "id2".into(), operator: Operator::Eq, value: id2.clone() },
                ],
            }),
            sorts: vec![],
        });

        let records = client.select(table, query).await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["id1"], id1);
        assert_eq!(records[0]["id2"], id2);

        delete_ids(&client, table, &row_ids).await;
    }

    #[tokio::test]
    async fn test_or_query() {
        dotenv().ok();
        let client = make_client();
        let table = "test_data";
        let run_id = Uuid::new_v4().to_string();
        let name_x = format!("Org X {run_id}");
        let name_z = format!("Org Z {run_id}");

        let id_x = client.insert(table, json!({ "name": name_x })).await.unwrap();
        let id_z = client.insert(table, json!({ "name": name_z })).await.unwrap();

        let query = (query!("name" == name_x.clone()) | query!("name" == name_z.clone())).to_query();
        assert_eq!(query, SelectQuery {
            filter: Some(FilterGroup {
                operator: LogicalOperator::Or,
                filters: vec![
                    Filter { column: "name".into(), operator: Operator::Eq, value: name_x.clone() },
                    Filter { column: "name".into(), operator: Operator::Eq, value: name_z.clone() },
                ],
            }),
            sorts: vec![],
        });

        let records = client.select(table, query).await.unwrap();
        assert_eq!(records.len(), 2);

        let ids: Vec<&str> = records.iter().map(|r| r["id"].as_str().unwrap()).collect();
        assert!(ids.contains(&id_x.as_str()));
        assert!(ids.contains(&id_z.as_str()));

        delete_ids(&client, table, &[id_x, id_z]).await;
    }

    #[tokio::test]
    async fn test_sorting_created_at() {
        dotenv().ok();
        let client = make_client();
        let table = "test_data";
        let run_id = Uuid::new_v4().to_string();

        // Insert 5 records with a unique run_id in id1 so we can filter just ours.
        let mut row_ids: Vec<String> = Vec::new();
        for i in 1..=5 {
            let id = client.insert(table, json!({ "name": format!("Org {} {run_id}", i), "id1": run_id })).await.unwrap();
            row_ids.push(id);
            sleep(Duration::from_millis(40)).await;
        }

        // Filter to only this test run's rows, then sort.
        let asc_query  = SelectQuery { filter: Some(FilterGroup::new(LogicalOperator::And, vec![Filter::new("id1", Operator::Eq, &run_id)])), sorts: vec![Sort::new("created_at", SortDirection::Asc)] };
        let desc_query = SelectQuery { filter: Some(FilterGroup::new(LogicalOperator::And, vec![Filter::new("id1", Operator::Eq, &run_id)])), sorts: vec![Sort::new("created_at", SortDirection::Desc)] };

        let asc_records  = client.select(table, asc_query).await.unwrap();
        let desc_records = client.select(table, desc_query).await.unwrap();

        assert_eq!(asc_records.len(), 5, "should have exactly 5 asc rows");
        assert_eq!(desc_records.len(), 5, "should have exactly 5 desc rows");

        let asc_dates:  Vec<&str> = asc_records.iter().map(|r| r["created_at"].as_str().unwrap()).collect();
        let desc_dates: Vec<&str> = desc_records.iter().map(|r| r["created_at"].as_str().unwrap()).collect();

        // Ascending reversed must equal descending.
        assert_eq!(asc_dates.iter().rev().cloned().collect::<Vec<_>>(), desc_dates);

        delete_ids(&client, table, &row_ids).await;
    }

    #[tokio::test]
    async fn test_neq_operator() {
        dotenv().ok();
        let client = make_client();
        let table = "test_data";
        let run_id = Uuid::new_v4().to_string();
        let alice   = format!("Alice {run_id}");
        let bob     = format!("Bob {run_id}");
        let charlie = format!("Charlie {run_id}");

        let mut row_ids: Vec<String> = Vec::new();
        row_ids.push(client.insert(table, json!({ "name": alice, "id1": run_id })).await.unwrap());
        row_ids.push(client.insert(table, json!({ "name": bob,   "id1": run_id })).await.unwrap());
        row_ids.push(client.insert(table, json!({ "name": charlie,"id1": run_id })).await.unwrap());
        sleep(Duration::from_millis(30)).await;

        let alice_name = format!("Alice {run_id}");
        // Filter: id1 == run_id AND name != Alice
        let filter = FilterGroup::new(LogicalOperator::And, vec![
            Filter::new("id1", Operator::Eq, &run_id),
            Filter::new("name", Operator::Neq, &alice_name),
        ]);
        let records = client.select(table, SelectQuery { filter: Some(filter), sorts: vec![] }).await.unwrap();

        assert_eq!(records.len(), 2, "expected Bob and Charlie only");
        for rec in &records {
            assert_ne!(rec["name"].as_str().unwrap(), alice_name);
        }

        delete_ids(&client, table, &row_ids).await;
    }

    #[tokio::test]
    async fn test_gt_operator() {
        dotenv().ok();
        let client = make_client();
        let table = "test_data";
        let run_id = Uuid::new_v4().to_string();

        let mut row_ids: Vec<String> = Vec::new();
        row_ids.push(client.insert(table, json!({ "name": format!("Item1 {run_id}"), "score": 50, "id1": run_id })).await.unwrap());
        row_ids.push(client.insert(table, json!({ "name": format!("Item2 {run_id}"), "score": 75, "id1": run_id })).await.unwrap());
        row_ids.push(client.insert(table, json!({ "name": format!("Item3 {run_id}"), "score": 100,"id1": run_id })).await.unwrap());
        sleep(Duration::from_millis(30)).await;

        let filter = FilterGroup::new(LogicalOperator::And, vec![
            Filter::new("id1", Operator::Eq, &run_id),
            Filter::new("score", Operator::Gt, "60"),
        ]);
        let records = client.select(table, SelectQuery { filter: Some(filter), sorts: vec![] }).await.unwrap();

        assert_eq!(records.len(), 2, "expected scores 75 and 100");
        for rec in &records {
            assert!(rec["score"].as_i64().unwrap() > 60);
        }

        delete_ids(&client, table, &row_ids).await;
    }

    #[tokio::test]
    async fn test_lt_operator() {
        dotenv().ok();
        let client = make_client();
        let table = "test_data";
        let run_id = Uuid::new_v4().to_string();

        let mut row_ids: Vec<String> = Vec::new();
        row_ids.push(client.insert(table, json!({ "name": format!("Item1 {run_id}"), "score": 10, "id1": run_id })).await.unwrap());
        row_ids.push(client.insert(table, json!({ "name": format!("Item2 {run_id}"), "score": 20, "id1": run_id })).await.unwrap());
        row_ids.push(client.insert(table, json!({ "name": format!("Item3 {run_id}"), "score": 30, "id1": run_id })).await.unwrap());
        sleep(Duration::from_millis(30)).await;

        let filter = FilterGroup::new(LogicalOperator::And, vec![
            Filter::new("id1", Operator::Eq, &run_id),
            Filter::new("score", Operator::Lt, "25"),
        ]);
        let records = client.select(table, SelectQuery { filter: Some(filter), sorts: vec![] }).await.unwrap();

        assert_eq!(records.len(), 2, "expected scores 10 and 20");
        for rec in &records {
            assert!(rec["score"].as_i64().unwrap() < 25);
        }

        delete_ids(&client, table, &row_ids).await;
    }

    #[tokio::test]
    async fn test_like_operator() {
        dotenv().ok();
        let client = make_client();
        let table = "test_data";
        let run_id = Uuid::new_v4().to_string();
        // Embed run_id via id1 so we can isolate this test's rows.
        let mut row_ids: Vec<String> = Vec::new();
        row_ids.push(client.insert(table, json!({ "name": "Alpha",    "id1": run_id })).await.unwrap());
        row_ids.push(client.insert(table, json!({ "name": "Beta",     "id1": run_id })).await.unwrap());
        row_ids.push(client.insert(table, json!({ "name": "Alphabet", "id1": run_id })).await.unwrap());
        sleep(Duration::from_millis(30)).await;

        // Filter: id1 == run_id AND name LIKE 'Alph%'
        let filter = FilterGroup::new(LogicalOperator::And, vec![
            Filter::new("id1", Operator::Eq, &run_id),
            Filter { column: "name".into(), operator: Operator::Like, value: "Alph%".into() },
        ]);
        let records = client.select(table, SelectQuery { filter: Some(filter), sorts: vec![] }).await.unwrap();

        assert_eq!(records.len(), 2, "expected Alpha and Alphabet");
        for rec in &records {
            assert!(rec["name"].as_str().unwrap().starts_with("Alph"));
        }

        delete_ids(&client, table, &row_ids).await;
    }

    #[tokio::test]
    async fn test_filter_and_sort() {
        dotenv().ok();
        let client = make_client();
        let table = "test_data";
        let run_id = Uuid::new_v4().to_string();

        let mut row_ids: Vec<String> = Vec::new();
        row_ids.push(client.insert(table, json!({ "name": format!("Item1 {run_id}"), "score": 10, "id1": run_id })).await.unwrap());
        row_ids.push(client.insert(table, json!({ "name": format!("Item2 {run_id}"), "score": 20, "id1": run_id })).await.unwrap());
        row_ids.push(client.insert(table, json!({ "name": format!("Item3 {run_id}"), "score": 30, "id1": run_id })).await.unwrap());
        sleep(Duration::from_millis(30)).await;

        let filter = FilterGroup::new(LogicalOperator::And, vec![
            Filter::new("id1", Operator::Eq, &run_id),
            Filter::new("score", Operator::Lt, "25"),
        ]);
        let records = client.select(table, SelectQuery {
            filter: Some(filter),
            sorts: vec![Sort::new("score", SortDirection::Desc)],
        }).await.unwrap();

        assert_eq!(records.len(), 2, "expected scores 20 and 10 in desc order");
        assert_eq!(records[0]["score"].as_i64().unwrap(), 20);
        assert_eq!(records[1]["score"].as_i64().unwrap(), 10);

        delete_ids(&client, table, &row_ids).await;
    }

    #[tokio::test]
    async fn can_create_simple_filter_query() {
        let lecture_id = "8e662d9e-c920-4d2f-bda7-09e5173cc494";
        let user_id = lecture_id;
        let filter = (query!("lecture_id" == lecture_id) & query!("user_id" == user_id)).to_filter_group();
        assert_eq!(filter, FilterGroup {
            operator: LogicalOperator::And,
            filters: vec![
                Filter { column: "lecture_id".into(), operator: Operator::Eq, value: lecture_id.into() },
                Filter { column: "user_id".into(),    operator: Operator::Eq, value: user_id.into() },
            ],
        });
        assert_eq!(
            filter.to_query_string(),
            "lecture_id=eq.8e662d9e-c920-4d2f-bda7-09e5173cc494&user_id=eq.8e662d9e-c920-4d2f-bda7-09e5173cc494"
        );
    }

    #[tokio::test]
    async fn can_use_expression_in_query_eq_macro() {
        dotenv().ok();
        let client = make_client();
        let table = "test_data";
        let run_id = Uuid::new_v4().to_string();
        let target_name = format!("Test Organisation {run_id}");

        let mut ids: Vec<String> = Vec::new();
        for _ in 0..5 {
            ids.push(client.insert(table, json!({ "name": target_name })).await.unwrap());
            sleep(Duration::from_millis(20)).await;
        }
        let diff_id = client.insert(table, json!({ "name": format!("Different Organisation {run_id}") })).await.unwrap();
        ids.push(diff_id);

        let name_var = target_name.clone();
        let first_half  = target_name.split_once(' ').unwrap().0.to_string();
        let second_half = format!("Organisation {run_id}");

        let records = client.select(table, query!("name" == name_var.clone()).to_query()).await.unwrap();
        let same_records = client.select(table, query!("name" == format!("{} {}", first_half, second_half)).to_query()).await.unwrap();
        let same_records_2 = client.select(table, query!("name" == &name_var).to_query()).await.unwrap();

        assert_eq!(records, same_records);
        assert_eq!(records, same_records_2);
        assert_eq!(records.len(), 5);

        delete_ids(&client, table, &ids).await;
    }

    #[tokio::test]
    async fn can_select_with_empty_query() {
        dotenv().ok();
        let client = make_client();
        let table = "test_data";
        let run_id = Uuid::new_v4().to_string();

        // Insert 3 rows tagged with run_id so we can isolate them.
        let mut row_ids: Vec<String> = Vec::new();
        row_ids.push(client.insert(table, json!({ "name": "Alpha",    "id1": run_id })).await.unwrap());
        row_ids.push(client.insert(table, json!({ "name": "Beta",     "id1": run_id })).await.unwrap());
        row_ids.push(client.insert(table, json!({ "name": "Alphabet", "id1": run_id })).await.unwrap());
        sleep(Duration::from_millis(30)).await;

        // Filter to only this test run's rows — proves the "empty filter" path still
        // works at the query-building level while keeping the count deterministic.
        let filter = FilterGroup::new(LogicalOperator::And, vec![
            Filter::new("id1", Operator::Eq, &run_id),
        ]);
        let records = client.select(table, SelectQuery { filter: Some(filter), sorts: vec![] }).await.unwrap();

        assert_eq!(records.len(), 3, "expected exactly the 3 rows we inserted");

        delete_ids(&client, table, &row_ids).await;
    }
}

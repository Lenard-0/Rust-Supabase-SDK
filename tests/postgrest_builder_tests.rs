//! End-to-end integration tests for the modern `PostgrestBuilder` API.
//!
//! These run against the live Supabase instance pointed to by `.env`. Each
//! test scopes its rows by a unique `run_id` placed in the `id1` column so
//! parallel test runs don't interfere.
//!
//! Schema assumed for `test_data`:
//!   id          uuid primary key default gen_random_uuid()
//!   name        text
//!   category    text
//!   score       int (or numeric)
//!   id1         text   -- used as run_id tag for test isolation
//!   id2         text
//!   created_at  timestamptz default now()
//!
//! If your `test_data` is missing any of these columns, the tests that need
//! them will fail with a Postgrest 42703 (undefined_column) error.

#![allow(clippy::unwrap_used)]

use dotenv::dotenv;
use rust_supabase_sdk::postgrest::CountMode;
use rust_supabase_sdk::{SupabaseClient, SupabaseError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use uuid::Uuid;

const TABLE: &str = "test_data";

fn make_client() -> SupabaseClient {
    SupabaseClient::new(
        env::var("SUPABASE_URL").expect("SUPABASE_URL not set"),
        env::var("SUPABASE_API_KEY").expect("SUPABASE_API_KEY not set"),
        None,
    )
}

/// Helper: insert a batch of rows tagged with the same `run_id` and return
/// the inserted row IDs. Each row is given `id1 = run_id`.
async fn insert_tagged(
    client: &SupabaseClient,
    run_id: &str,
    rows: Vec<Value>,
) -> Vec<String> {
    let mut tagged = Vec::new();
    for mut row in rows {
        if let Some(obj) = row.as_object_mut() {
            obj.insert("id1".to_string(), Value::String(run_id.to_string()));
        }
        tagged.push(row);
    }
    let inserted: Vec<Value> = client
        .from(TABLE)
        .insert(json!(tagged))
        .select_returning("*")
        .execute()
        .await
        .expect("insert should succeed");
    inserted
        .into_iter()
        .map(|v| v["id"].as_str().unwrap().to_string())
        .collect()
}

/// Helper: delete all rows tagged with a `run_id`.
async fn cleanup(client: &SupabaseClient, run_id: &str) {
    let _ = client
        .from(TABLE)
        .delete()
        .eq("id1", run_id)
        .execute()
        .await;
}

// ===========================================================================
// SELECT — column projection
// ===========================================================================

#[tokio::test]
async fn select_star_returns_all_inserted_rows() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();

    insert_tagged(
        &client,
        &run_id,
        vec![
            json!({"name": "alpha"}),
            json!({"name": "beta"}),
            json!({"name": "gamma"}),
        ],
    )
    .await;

    let rows: Vec<Value> = client
        .from(TABLE)
        .select("*")
        .eq("id1", &run_id)
        .execute()
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);
    cleanup(&client, &run_id).await;
}

#[tokio::test]
async fn select_specific_columns_only() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();
    insert_tagged(&client, &run_id, vec![json!({"name": "only-me", "category": "cat-a"})])
        .await;

    let rows: Vec<Value> = client
        .from(TABLE)
        .select("name,category")
        .eq("id1", &run_id)
        .execute()
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["name"], "only-me");
    assert_eq!(rows[0]["category"], "cat-a");
    // id was NOT requested — must be absent.
    assert!(rows[0].get("id").is_none() || rows[0]["id"].is_null());
    cleanup(&client, &run_id).await;
}

// ===========================================================================
// FILTER — every comparison operator
// ===========================================================================

#[tokio::test]
async fn eq_neq_filters() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();
    insert_tagged(
        &client,
        &run_id,
        vec![
            json!({"name": "alice"}),
            json!({"name": "bob"}),
            json!({"name": "carol"}),
        ],
    )
    .await;

    let alice_only: Vec<Value> = client
        .from(TABLE)
        .select("name")
        .eq("id1", &run_id)
        .eq("name", "alice")
        .execute()
        .await
        .unwrap();
    assert_eq!(alice_only.len(), 1);
    assert_eq!(alice_only[0]["name"], "alice");

    let not_alice: Vec<Value> = client
        .from(TABLE)
        .select("name")
        .eq("id1", &run_id)
        .neq("name", "alice")
        .execute()
        .await
        .unwrap();
    assert_eq!(not_alice.len(), 2);
    for r in &not_alice {
        assert_ne!(r["name"], "alice");
    }
    cleanup(&client, &run_id).await;
}

#[tokio::test]
async fn gt_gte_lt_lte_filters() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();
    insert_tagged(
        &client,
        &run_id,
        vec![
            json!({"score": 10}),
            json!({"score": 20}),
            json!({"score": 30}),
            json!({"score": 40}),
        ],
    )
    .await;

    let gt: Vec<Value> = client
        .from(TABLE).select("score")
        .eq("id1", &run_id).gt("score", 25)
        .execute().await.unwrap();
    assert_eq!(gt.len(), 2); // 30, 40

    let gte: Vec<Value> = client
        .from(TABLE).select("score")
        .eq("id1", &run_id).gte("score", 30)
        .execute().await.unwrap();
    assert_eq!(gte.len(), 2); // 30, 40

    let lt: Vec<Value> = client
        .from(TABLE).select("score")
        .eq("id1", &run_id).lt("score", 25)
        .execute().await.unwrap();
    assert_eq!(lt.len(), 2); // 10, 20

    let lte: Vec<Value> = client
        .from(TABLE).select("score")
        .eq("id1", &run_id).lte("score", 20)
        .execute().await.unwrap();
    assert_eq!(lte.len(), 2); // 10, 20

    cleanup(&client, &run_id).await;
}

#[tokio::test]
async fn like_and_ilike_patterns() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();
    insert_tagged(
        &client,
        &run_id,
        vec![
            json!({"name": "Alpha"}),
            json!({"name": "alphabet"}),
            json!({"name": "Beta"}),
        ],
    )
    .await;

    // LIKE is case-sensitive. PostgREST uses * (not %) in the URL syntax.
    let like_match: Vec<Value> = client
        .from(TABLE).select("name")
        .eq("id1", &run_id).like("name", "Alpha*")
        .execute().await.unwrap();
    assert_eq!(like_match.len(), 1);
    assert_eq!(like_match[0]["name"], "Alpha");

    // ILIKE is case-insensitive.
    let ilike_match: Vec<Value> = client
        .from(TABLE).select("name")
        .eq("id1", &run_id).ilike("name", "ALPHA*")
        .execute().await.unwrap();
    assert_eq!(ilike_match.len(), 2);

    cleanup(&client, &run_id).await;
}

#[tokio::test]
async fn in_filter_matches_listed_values() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();
    insert_tagged(
        &client,
        &run_id,
        vec![
            json!({"name": "a"}),
            json!({"name": "b"}),
            json!({"name": "c"}),
            json!({"name": "d"}),
        ],
    )
    .await;

    let rows: Vec<Value> = client
        .from(TABLE)
        .select("name")
        .eq("id1", &run_id)
        .in_("name", ["a", "c"])
        .execute()
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    let names: Vec<&str> = rows.iter().map(|r| r["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"a"));
    assert!(names.contains(&"c"));

    cleanup(&client, &run_id).await;
}

#[tokio::test]
async fn is_null_filter() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();
    // Both rows must have the same key set for a PostgREST batch insert
    // — pass `category: null` explicitly on the no-category row.
    insert_tagged(
        &client,
        &run_id,
        vec![
            json!({"name": "has-cat", "category": "x"}),
            json!({"name": "no-cat", "category": Value::Null}),
        ],
    )
    .await;

    let null_rows: Vec<Value> = client
        .from(TABLE)
        .select("name,category")
        .eq("id1", &run_id)
        .is("category", "null")
        .execute()
        .await
        .unwrap();
    assert_eq!(null_rows.len(), 1);
    assert_eq!(null_rows[0]["name"], "no-cat");

    cleanup(&client, &run_id).await;
}

#[tokio::test]
async fn not_filter_negates() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();
    insert_tagged(
        &client,
        &run_id,
        vec![json!({"name": "keep"}), json!({"name": "drop"})],
    )
    .await;

    let kept: Vec<Value> = client
        .from(TABLE)
        .select("name")
        .eq("id1", &run_id)
        .not("name", "eq", "drop")
        .execute()
        .await
        .unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0]["name"], "keep");

    cleanup(&client, &run_id).await;
}

#[tokio::test]
async fn or_filter_combines_alternatives() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();
    insert_tagged(
        &client,
        &run_id,
        vec![
            json!({"name": "x", "score": 1}),
            json!({"name": "y", "score": 100}),
            json!({"name": "z", "score": 50}),
        ],
    )
    .await;

    // name=x OR score>=100. Two rows match.
    let rows: Vec<Value> = client
        .from(TABLE)
        .select("name,score")
        .eq("id1", &run_id)
        .or("name.eq.x,score.gte.100")
        .execute()
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);

    cleanup(&client, &run_id).await;
}

#[tokio::test]
async fn match_filter_applies_each_key() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();
    insert_tagged(
        &client,
        &run_id,
        vec![
            json!({"name": "a", "category": "1"}),
            json!({"name": "a", "category": "2"}),
            json!({"name": "b", "category": "1"}),
        ],
    )
    .await;

    let rows: Vec<Value> = client
        .from(TABLE)
        .select("name,category")
        .eq("id1", &run_id)
        .match_(json!({"name": "a", "category": "1"}))
        .execute()
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);

    cleanup(&client, &run_id).await;
}

// ===========================================================================
// MODIFIERS — order, limit, offset, range
// ===========================================================================

#[tokio::test]
async fn order_asc_and_desc() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();
    insert_tagged(
        &client,
        &run_id,
        vec![
            json!({"score": 30}),
            json!({"score": 10}),
            json!({"score": 20}),
        ],
    )
    .await;

    let asc: Vec<Value> = client
        .from(TABLE).select("score")
        .eq("id1", &run_id).order("score", true)
        .execute().await.unwrap();
    let asc_scores: Vec<i64> = asc.iter().map(|r| r["score"].as_i64().unwrap()).collect();
    assert_eq!(asc_scores, vec![10, 20, 30]);

    let desc: Vec<Value> = client
        .from(TABLE).select("score")
        .eq("id1", &run_id).order("score", false)
        .execute().await.unwrap();
    let desc_scores: Vec<i64> = desc.iter().map(|r| r["score"].as_i64().unwrap()).collect();
    assert_eq!(desc_scores, vec![30, 20, 10]);

    cleanup(&client, &run_id).await;
}

#[tokio::test]
async fn limit_caps_returned_rows() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();
    insert_tagged(
        &client,
        &run_id,
        (0..5).map(|i| json!({"score": i})).collect(),
    )
    .await;

    let rows: Vec<Value> = client
        .from(TABLE)
        .select("score")
        .eq("id1", &run_id)
        .limit(2)
        .execute()
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);

    cleanup(&client, &run_id).await;
}

#[tokio::test]
async fn offset_skips_rows() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();
    insert_tagged(
        &client,
        &run_id,
        (0..5i64).map(|i| json!({"score": i})).collect(),
    )
    .await;

    let rows: Vec<Value> = client
        .from(TABLE)
        .select("score")
        .eq("id1", &run_id)
        .order("score", true)
        .offset(3)
        .execute()
        .await
        .unwrap();
    // 5 rows total, skip 3 → 2 remaining.
    assert_eq!(rows.len(), 2);

    cleanup(&client, &run_id).await;
}

#[tokio::test]
async fn range_returns_inclusive_slice() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();
    insert_tagged(
        &client,
        &run_id,
        (0..6i64).map(|i| json!({"score": i})).collect(),
    )
    .await;

    let rows: Vec<Value> = client
        .from(TABLE)
        .select("score")
        .eq("id1", &run_id)
        .order("score", true)
        .range(1, 3)
        .execute()
        .await
        .unwrap();
    // range(1, 3) inclusive → 3 rows.
    assert_eq!(rows.len(), 3);
    let scores: Vec<i64> = rows.iter().map(|r| r["score"].as_i64().unwrap()).collect();
    assert_eq!(scores, vec![1, 2, 3]);

    cleanup(&client, &run_id).await;
}

// ===========================================================================
// COUNT
// ===========================================================================

#[tokio::test]
async fn count_exact_returns_total_via_content_range() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();
    insert_tagged(
        &client,
        &run_id,
        (0..4).map(|i| json!({"score": i})).collect(),
    )
    .await;

    let (rows, count): (Vec<Value>, Option<u64>) = client
        .from(TABLE)
        .select("*")
        .eq("id1", &run_id)
        .count(CountMode::Exact)
        .execute_with_count()
        .await
        .unwrap();
    assert_eq!(rows.len(), 4);
    assert_eq!(count, Some(4));

    cleanup(&client, &run_id).await;
}

// ===========================================================================
// SINGLE / MAYBE_SINGLE
// ===========================================================================

#[tokio::test]
async fn single_returns_one_row() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();
    insert_tagged(&client, &run_id, vec![json!({"name": "uniq"})]).await;

    let row: Value = client
        .from(TABLE)
        .select("*")
        .eq("id1", &run_id)
        .single()
        .execute()
        .await
        .unwrap();
    assert_eq!(row["name"], "uniq");

    cleanup(&client, &run_id).await;
}

#[tokio::test]
async fn single_zero_rows_returns_not_found() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();
    // No insert — query for a non-existent run_id.

    let err = client
        .from(TABLE)
        .select("*")
        .eq("id1", &run_id)
        .single()
        .execute()
        .await
        .expect_err("should be NotFound");
    matches!(err, SupabaseError::NotFound { .. });
}

#[tokio::test]
async fn single_multiple_rows_yields_unexpected() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();
    insert_tagged(
        &client,
        &run_id,
        vec![json!({"name": "a"}), json!({"name": "b"})],
    )
    .await;

    let err = client
        .from(TABLE)
        .select("*")
        .eq("id1", &run_id)
        .single()
        .execute()
        .await
        .expect_err("multiple rows should error");
    matches!(err, SupabaseError::Unexpected(_));

    cleanup(&client, &run_id).await;
}

#[tokio::test]
async fn maybe_single_handles_zero_and_one() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();

    // Zero → Ok(None).
    let none: Option<Value> = client
        .from(TABLE)
        .select("*")
        .eq("id1", &run_id)
        .maybe_single()
        .execute()
        .await
        .unwrap();
    assert!(none.is_none());

    // One → Ok(Some(_)).
    insert_tagged(&client, &run_id, vec![json!({"name": "only"})]).await;
    let some: Option<Value> = client
        .from(TABLE)
        .select("*")
        .eq("id1", &run_id)
        .maybe_single()
        .execute()
        .await
        .unwrap();
    assert!(some.is_some());

    cleanup(&client, &run_id).await;
}

// ===========================================================================
// INSERT / UPDATE / UPSERT / DELETE
// ===========================================================================

#[tokio::test]
async fn insert_single_row_returns_representation() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();

    let inserted: Vec<Value> = client
        .from(TABLE)
        .insert(json!({"name": "fresh", "id1": run_id}))
        .select_returning("*")
        .execute()
        .await
        .unwrap();
    assert_eq!(inserted.len(), 1);
    assert_eq!(inserted[0]["name"], "fresh");

    cleanup(&client, &run_id).await;
}

#[tokio::test]
async fn insert_batch_returns_all() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();

    let inserted: Vec<Value> = client
        .from(TABLE)
        .insert(json!([
            {"name": "a", "id1": run_id},
            {"name": "b", "id1": run_id},
            {"name": "c", "id1": run_id},
        ]))
        .select_returning("*")
        .execute()
        .await
        .unwrap();
    assert_eq!(inserted.len(), 3);

    cleanup(&client, &run_id).await;
}

#[tokio::test]
async fn update_modifies_matched_rows() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();
    insert_tagged(
        &client,
        &run_id,
        vec![json!({"name": "before"}), json!({"name": "before"})],
    )
    .await;

    let updated: Vec<Value> = client
        .from(TABLE)
        .update(json!({"name": "after"}))
        .eq("id1", &run_id)
        .select_returning("*")
        .execute()
        .await
        .unwrap();
    assert_eq!(updated.len(), 2);
    for r in &updated {
        assert_eq!(r["name"], "after");
    }

    cleanup(&client, &run_id).await;
}

#[tokio::test]
async fn delete_with_filter_removes_only_matched() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();
    insert_tagged(
        &client,
        &run_id,
        vec![
            json!({"name": "keep"}),
            json!({"name": "drop"}),
            json!({"name": "drop"}),
        ],
    )
    .await;

    client
        .from(TABLE)
        .delete()
        .eq("id1", &run_id)
        .eq("name", "drop")
        .execute()
        .await
        .unwrap();

    let remaining: Vec<Value> = client
        .from(TABLE)
        .select("name")
        .eq("id1", &run_id)
        .execute()
        .await
        .unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0]["name"], "keep");

    cleanup(&client, &run_id).await;
}

#[tokio::test]
async fn upsert_inserts_then_updates_same_id() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();
    let row_id = Uuid::new_v4().to_string();

    // First upsert: insert.
    let first: Vec<Value> = client
        .from(TABLE)
        .upsert(json!({"id": row_id, "name": "v1", "id1": run_id}))
        .select_returning("*")
        .execute()
        .await
        .unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0]["name"], "v1");

    // Second upsert: update.
    let second: Vec<Value> = client
        .from(TABLE)
        .upsert(json!({"id": row_id, "name": "v2", "id1": run_id}))
        .select_returning("*")
        .execute()
        .await
        .unwrap();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0]["name"], "v2");

    cleanup(&client, &run_id).await;
}

// ===========================================================================
// .returns::<T>() — typed deserialization
// ===========================================================================

#[derive(Debug, Deserialize, Serialize)]
struct TestRow {
    id: String,
    name: Option<String>,
}

#[tokio::test]
async fn returns_typed_deserializes_into_struct() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();
    insert_tagged(&client, &run_id, vec![json!({"name": "typed"})]).await;

    let rows: Vec<TestRow> = client
        .from(TABLE)
        .select("id,name")
        .eq("id1", &run_id)
        .returns::<TestRow>()
        .execute()
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name.as_deref(), Some("typed"));
    assert!(!rows[0].id.is_empty());

    cleanup(&client, &run_id).await;
}

// ===========================================================================
// Chained filters and IntoFuture await syntax
// ===========================================================================

#[tokio::test]
async fn chained_filters_combine_as_and() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();
    insert_tagged(
        &client,
        &run_id,
        vec![
            json!({"name": "x", "score": 50}),
            json!({"name": "x", "score": 90}),
            json!({"name": "y", "score": 90}),
        ],
    )
    .await;

    let rows: Vec<Value> = client
        .from(TABLE)
        .select("name,score")
        .eq("id1", &run_id)
        .eq("name", "x")
        .gte("score", 80)
        .execute()
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["name"], "x");
    assert_eq!(rows[0]["score"], 90);

    cleanup(&client, &run_id).await;
}

#[tokio::test]
async fn await_directly_via_into_future() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();
    insert_tagged(&client, &run_id, vec![json!({"name": "future"})]).await;

    // .await on PostgrestBuilder (no explicit .execute()).
    let rows: Vec<Value> = client
        .from(TABLE)
        .select("*")
        .eq("id1", &run_id)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);

    cleanup(&client, &run_id).await;
}

// ===========================================================================
// Edge cases
// ===========================================================================

#[tokio::test]
async fn empty_result_is_an_empty_vec_not_an_error() {
    dotenv().ok();
    let client = make_client();
    let nonexistent = Uuid::new_v4().to_string();
    let rows: Vec<Value> = client
        .from(TABLE)
        .select("*")
        .eq("id1", &nonexistent)
        .execute()
        .await
        .unwrap();
    assert!(rows.is_empty());
}

#[tokio::test]
async fn filter_with_special_characters_in_value() {
    dotenv().ok();
    let client = make_client();
    let run_id = Uuid::new_v4().to_string();
    let weird = "O'Brien & Sons (Co.)";
    insert_tagged(&client, &run_id, vec![json!({"name": weird})]).await;

    let rows: Vec<Value> = client
        .from(TABLE)
        .select("name")
        .eq("id1", &run_id)
        .eq("name", weird)
        .execute()
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["name"], weird);

    cleanup(&client, &run_id).await;
}

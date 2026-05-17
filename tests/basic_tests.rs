//! Live-DB smoke tests for the supabase-js-style builder API: insert →
//! select → update → delete, plus upsert. Run against the project specified
//! by `SUPABASE_URL` + `SUPABASE_API_KEY` in `.env`.

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use dotenv::dotenv;
    use rust_supabase_sdk::SupabaseClient;
    use serde_json::{json, Value};
    use uuid::Uuid;

    fn client() -> SupabaseClient {
        dotenv().ok();
        SupabaseClient::new(
            std::env::var("SUPABASE_URL").unwrap(),
            std::env::var("SUPABASE_API_KEY").unwrap(),
            None,
        )
    }

    #[tokio::test]
    async fn can_initialise_supabase_client() {
        let _c = client();
    }

    #[tokio::test]
    async fn can_create_get_update_and_remove() {
        let c = client();
        let run = Uuid::new_v4().to_string();
        let name = format!("Test Org {run}");

        // Insert and read the inserted id back.
        let inserted: Vec<Value> = c
            .from("organisations")
            .insert(json!({ "name": name }))
            .select_returning("id,name")
            .await
            .unwrap();
        assert_eq!(inserted.len(), 1);
        let record_id = inserted[0]["id"].as_str().unwrap().to_string();

        // Select by id.
        let row: Value = c
            .from("organisations")
            .select("*")
            .eq("id", &record_id)
            .single()
            .await
            .unwrap();
        assert_eq!(row["name"], name);

        // Update.
        let updated_name = format!("Updated {run}");
        let _: Vec<Value> = c
            .from("organisations")
            .update(json!({ "name": &updated_name }))
            .eq("id", &record_id)
            .await
            .unwrap();

        let row: Value = c
            .from("organisations")
            .select("name")
            .eq("id", &record_id)
            .single()
            .await
            .unwrap();
        assert_eq!(row["name"], updated_name);

        // Delete.
        let _: Vec<Value> = c
            .from("organisations")
            .delete()
            .eq("id", &record_id)
            .await
            .unwrap();

        let missing = c
            .from("organisations")
            .select("*")
            .eq("id", &record_id)
            .single()
            .await;
        assert!(missing.is_err(), "row should be gone after delete");
    }

    #[tokio::test]
    async fn can_upsert() {
        let c = client();
        let run = Uuid::new_v4().to_string();
        let id = Uuid::new_v4().to_string();
        let name = format!("Test Org {run}");

        // First upsert — inserts.
        let _: Vec<Value> = c
            .from("organisations")
            .upsert(json!({ "id": &id, "name": &name }))
            .on_conflict("id")
            .await
            .unwrap();

        let row: Value = c
            .from("organisations")
            .select("*")
            .eq("id", &id)
            .single()
            .await
            .unwrap();
        assert_eq!(row["name"], name);

        // Second upsert with same id — updates.
        let updated = format!("Updated {run}");
        let _: Vec<Value> = c
            .from("organisations")
            .upsert(json!({ "id": &id, "name": &updated }))
            .on_conflict("id")
            .await
            .unwrap();

        let row: Value = c
            .from("organisations")
            .select("*")
            .eq("id", &id)
            .single()
            .await
            .unwrap();
        assert_eq!(row["name"], updated);

        // Cleanup.
        let _: Vec<Value> = c
            .from("organisations")
            .delete()
            .eq("id", &id)
            .await
            .unwrap();
    }
}

//! Live-DB integration tests for the type-safe query path:
//! `SupabaseClient::from_row::<R>()` + `Column<R, V>` + `TypedBuilder<R>`.
//!
//! Mirrors the conventions in `basic_tests.rs`: dotenv-loaded client,
//! UUID-tagged per-run isolation, explicit cleanup by inserted id. Tests
//! run in parallel against a shared Supabase project, so every test
//! tags its rows with its own `run_id` and deletes exactly what it
//! inserted — no `delete().eq("name", ...)` shotgun.
//!
//! Insert paths still go through the **string** builder because typed
//! insert isn't built yet; the typed surface under test is the
//! select/filter/order/limit/single/maybe_single/count chain.

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use dotenv::dotenv;
    use rust_supabase_sdk::postgrest::{Column, CountMode};
    use rust_supabase_sdk::{Row, SupabaseClient};
    use serde::{Deserialize, Serialize};
    use serde_json::{json, Value};
    use uuid::Uuid;

    // Mirrors the live `organisations` schema for fields we actually use in
    // tests. We deliberately stop at `id` and `name` — anything else risks
    // a deserialization failure if the column is absent or shaped
    // differently than we guessed. `basic_tests.rs` only ever touches
    // these two, so they're the safe floor.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Organisations {
        id: String,
        name: String,
    }

    impl Row for Organisations {
        const TABLE: &'static str = "organisations";
    }

    #[allow(non_upper_case_globals)]
    impl Organisations {
        pub const id: Column<Organisations, String> = Column::new("id");
        pub const name: Column<Organisations, String> = Column::new("name");
    }

    fn client() -> SupabaseClient {
        dotenv().ok();
        SupabaseClient::new(
            std::env::var("SUPABASE_URL").unwrap(),
            std::env::var("SUPABASE_API_KEY").unwrap(),
            None,
        )
    }

    /// Insert one row with `name = name` via the string API and return its
    /// generated `id`. Centralised so each test's setup is a one-liner.
    async fn insert_named(c: &SupabaseClient, name: &str) -> String {
        let inserted: Vec<Value> = c
            .from("organisations")
            .insert(json!({ "name": name }))
            .select_returning("id")
            .await
            .unwrap();
        inserted[0]["id"].as_str().unwrap().to_string()
    }

    /// Delete every row whose id is in `ids`. Best-effort: per-id so a
    /// single failure doesn't strand the rest.
    async fn cleanup(c: &SupabaseClient, ids: &[String]) {
        for id in ids {
            let _: Vec<Value> = c
                .from("organisations")
                .delete()
                .eq("id", id)
                .await
                .unwrap();
        }
    }

    // -----------------------------------------------------------------
    // 1. Typed select returns typed rows (not Vec<Value>).
    // -----------------------------------------------------------------
    #[tokio::test]
    async fn typed_select_returns_typed_rows() {
        let c = client();
        let run_id = Uuid::new_v4().to_string();
        let unique_name = format!("Typed select {run_id}");
        let mut ids: Vec<String> = Vec::new();
        ids.push(insert_named(&c, &unique_name).await);

        let rows: Vec<Organisations> = c
            .from_row::<Organisations>()
            .eq(Organisations::name, unique_name.clone())
            .execute()
            .await
            .unwrap();

        assert_eq!(rows.len(), 1);
        // Field access — not JSON indexing. This is the whole point of the
        // typed path: the compiler proves these fields exist.
        assert_eq!(rows[0].name, unique_name);
        assert_eq!(rows[0].id, ids[0]);

        cleanup(&c, &ids).await;
    }

    // -----------------------------------------------------------------
    // 2. .eq round-trip with multiple matching rows.
    // -----------------------------------------------------------------
    #[tokio::test]
    async fn typed_eq_filter_round_trip() {
        let c = client();
        let run_id = Uuid::new_v4().to_string();
        let unique_name = format!("Typed eq {run_id}");
        let mut ids: Vec<String> = Vec::new();
        for _ in 0..3 {
            ids.push(insert_named(&c, &unique_name).await);
        }

        let rows: Vec<Organisations> = c
            .from_row::<Organisations>()
            .eq(Organisations::name, unique_name.clone())
            .execute()
            .await
            .unwrap();

        assert_eq!(rows.len(), 3);
        for r in &rows {
            assert_eq!(r.name, unique_name);
        }

        cleanup(&c, &ids).await;
    }

    // -----------------------------------------------------------------
    // 3. .neq excludes matching rows.
    // -----------------------------------------------------------------
    #[tokio::test]
    async fn typed_neq_filter() {
        let c = client();
        let run_id = Uuid::new_v4().to_string();
        let name_a = format!("Typed neq A {run_id}");
        let name_b = format!("Typed neq B {run_id}");
        let mut ids: Vec<String> = Vec::new();
        ids.push(insert_named(&c, &name_a).await);
        ids.push(insert_named(&c, &name_a).await);
        ids.push(insert_named(&c, &name_b).await);

        // Scope by id-in (the rows we just inserted) and then neq on name —
        // otherwise we'd be filtering against the whole org table.
        let rows: Vec<Organisations> = c
            .from_row::<Organisations>()
            .in_(Organisations::id, ids.clone())
            .neq(Organisations::name, name_a.clone())
            .execute()
            .await
            .unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, name_b);

        cleanup(&c, &ids).await;
    }

    // -----------------------------------------------------------------
    // 4. .in_ matches a value-list filter.
    // -----------------------------------------------------------------
    #[tokio::test]
    async fn typed_in_filter() {
        let c = client();
        let run_id = Uuid::new_v4().to_string();
        let name_a = format!("Typed in A {run_id}");
        let name_b = format!("Typed in B {run_id}");
        let name_c = format!("Typed in C {run_id}");
        let mut ids: Vec<String> = Vec::new();
        ids.push(insert_named(&c, &name_a).await);
        ids.push(insert_named(&c, &name_b).await);
        ids.push(insert_named(&c, &name_c).await);

        // in_ on name returns whichever rows the run inserted with those
        // two names — combined with id-in scoping to be parallel-safe.
        let rows: Vec<Organisations> = c
            .from_row::<Organisations>()
            .in_(Organisations::id, ids.clone())
            .in_(Organisations::name, vec![name_a.clone(), name_b.clone()])
            .execute()
            .await
            .unwrap();

        assert_eq!(rows.len(), 2);
        let mut got: Vec<String> = rows.into_iter().map(|r| r.name).collect();
        got.sort();
        let mut want = vec![name_a, name_b];
        want.sort();
        assert_eq!(got, want);

        cleanup(&c, &ids).await;
    }

    // -----------------------------------------------------------------
    // 5. .order + .limit returns the right subset in the right order.
    // -----------------------------------------------------------------
    #[tokio::test]
    async fn typed_order_and_limit() {
        let c = client();
        let run_id = Uuid::new_v4().to_string();
        let mut ids: Vec<String> = Vec::new();
        let mut names: Vec<String> = Vec::new();
        // Names are lexicographically ordered "Org 1 …" < "Org 2 …" < …
        // because they share a constant prefix and a digit.
        for i in 1..=5 {
            let n = format!("Org {i} {run_id}");
            ids.push(insert_named(&c, &n).await);
            names.push(n);
        }

        let rows: Vec<Organisations> = c
            .from_row::<Organisations>()
            .in_(Organisations::id, ids.clone())
            .order(Organisations::name, true)
            .limit(3)
            .execute()
            .await
            .unwrap();

        assert_eq!(rows.len(), 3);
        // First three names ascending → "Org 1 …", "Org 2 …", "Org 3 …".
        let got: Vec<String> = rows.into_iter().map(|r| r.name).collect();
        assert_eq!(got, names[0..3].to_vec());

        cleanup(&c, &ids).await;
    }

    // -----------------------------------------------------------------
    // 6. .single returns the typed struct directly (not Vec<R>).
    // -----------------------------------------------------------------
    #[tokio::test]
    async fn typed_single_returns_one_typed_row() {
        let c = client();
        let run_id = Uuid::new_v4().to_string();
        let unique_name = format!("Typed single {run_id}");
        let mut ids: Vec<String> = Vec::new();
        ids.push(insert_named(&c, &unique_name).await);

        let row: Organisations = c
            .from_row::<Organisations>()
            .eq(Organisations::id, ids[0].clone())
            .single()
            .await
            .unwrap();

        // No `[0]` indexing — it's a single typed value.
        assert_eq!(row.name, unique_name);
        assert_eq!(row.id, ids[0]);

        cleanup(&c, &ids).await;
    }

    // -----------------------------------------------------------------
    // 7. .maybe_single returns None for zero rows.
    // -----------------------------------------------------------------
    #[tokio::test]
    async fn typed_maybe_single_returns_none_for_zero_rows() {
        let c = client();
        let run_id = Uuid::new_v4().to_string();
        let missing_name = format!("nonexistent-{run_id}");

        let row: Option<Organisations> = c
            .from_row::<Organisations>()
            .eq(Organisations::name, missing_name)
            .maybe_single()
            .await
            .unwrap();

        assert!(row.is_none());
    }

    // -----------------------------------------------------------------
    // 8. .count + .execute_with_count returns typed rows and a total.
    // -----------------------------------------------------------------
    #[tokio::test]
    async fn typed_execute_with_count() {
        let c = client();
        let run_id = Uuid::new_v4().to_string();
        let unique_name = format!("Typed count {run_id}");
        let mut ids: Vec<String> = Vec::new();
        for _ in 0..3 {
            ids.push(insert_named(&c, &unique_name).await);
        }

        let (rows, count) = c
            .from_row::<Organisations>()
            .eq(Organisations::name, unique_name.clone())
            .count(CountMode::Exact)
            .execute_with_count()
            .await
            .unwrap();

        // Exact count because the name is UUID-tagged and unique to this run.
        assert_eq!(rows.len(), 3);
        let count = count.expect("CountMode::Exact should set Content-Range total");
        assert!(count >= 3, "expected at least 3, got {count}");
        for r in &rows {
            assert_eq!(r.name, unique_name);
        }

        cleanup(&c, &ids).await;
    }

    // -----------------------------------------------------------------
    // 9. String API and typed API coexist on the same table / data.
    // -----------------------------------------------------------------
    #[tokio::test]
    async fn string_path_still_works_for_same_table() {
        let c = client();
        let run_id = Uuid::new_v4().to_string();
        let unique_name = format!("Typed coexist {run_id}");
        let mut ids: Vec<String> = Vec::new();
        for _ in 0..2 {
            ids.push(insert_named(&c, &unique_name).await);
        }

        // String API path.
        let string_rows: Vec<Value> = c
            .from("organisations")
            .select("*")
            .eq("name", &unique_name)
            .await
            .unwrap();
        assert_eq!(string_rows.len(), 2);
        for r in &string_rows {
            assert_eq!(r["name"].as_str().unwrap(), unique_name);
        }

        // Typed API path against the same rows.
        let typed_rows: Vec<Organisations> = c
            .from_row::<Organisations>()
            .eq(Organisations::name, unique_name.clone())
            .execute()
            .await
            .unwrap();
        assert_eq!(typed_rows.len(), 2);
        for r in &typed_rows {
            assert_eq!(r.name, unique_name);
        }

        cleanup(&c, &ids).await;
    }
}

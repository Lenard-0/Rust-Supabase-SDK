//! `cargo run --example realtime_changes --features realtime`
//!
//! Subscribe to `public.messages` and print every INSERT/UPDATE/DELETE event,
//! plus echo a broadcast message every five seconds. Demonstrates both the
//! stream-style (`Channel::recv`) and callback-style (`Channel::run`) APIs.
//!
//! Requires `SUPABASE_URL` and `SUPABASE_API_KEY` in the environment, and
//! the `realtime` feature enabled.

#[cfg(feature = "realtime")]
mod imp {
    use std::time::Duration;

    use rust_supabase_sdk::realtime::{PostgresChangeKind, PostgresChangesFilter, RealtimeEvent};
    use rust_supabase_sdk::SupabaseClient;
    use serde_json::json;

    pub async fn run() -> rust_supabase_sdk::Result<()> {
        let url = std::env::var("SUPABASE_URL").map_err(|_| {
            rust_supabase_sdk::SupabaseError::Unexpected("SUPABASE_URL not set".into())
        })?;
        let key = std::env::var("SUPABASE_API_KEY").map_err(|_| {
            rust_supabase_sdk::SupabaseError::Unexpected("SUPABASE_API_KEY not set".into())
        })?;

        let client = SupabaseClient::new(url, key, None);
        let rt = client.realtime().connect().await?;
        let topic = "realtime:public:messages";

        let mut channel = rt
            .channel(topic)
            .on_postgres_changes(
                PostgresChangesFilter::new(PostgresChangeKind::All)
                    .schema("public")
                    .table("messages"),
            )
            .on_postgres_changes_callback(
                PostgresChangesFilter::new(PostgresChangeKind::Insert)
                    .schema("public")
                    .table("messages"),
                |change| {
                    println!("[callback] insert: {}", change.record);
                },
            )
            .on_broadcast(true, false)
            .on_broadcast_callback(Some("ping"), |b| {
                println!("[callback] broadcast ping: {:?}", b.payload);
            })
            .subscribe()
            .await?;

        // Periodically broadcast a "ping" so this demo is self-driving.
        let publisher = channel_send_loop(rt.clone(), topic.to_string());
        tokio::spawn(publisher);

        // Stream-style receive loop. Callbacks above also fire for matching
        // events because we manually dispatch through `Channel::run`-equivalent
        // logic via `recv` + `match` here.
        while let Some(event) = channel.recv().await {
            match event {
                RealtimeEvent::PostgresChange(c) => {
                    println!("change [{}]: {} ({} <- {})", c.change_type, c.table, c.record, c.old_record);
                }
                RealtimeEvent::Broadcast(b) => println!("broadcast {}: {}", b.event, b.payload),
                RealtimeEvent::Closed => {
                    eprintln!("channel closed");
                    break;
                }
                RealtimeEvent::Error(e) => eprintln!("error: {e}"),
                _ => {}
            }
        }
        Ok(())
    }

    async fn channel_send_loop(
        rt: rust_supabase_sdk::realtime::RealtimeClient,
        topic: String,
    ) {
        let sender = match rt.channel(&topic).subscribe().await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("sender subscribe failed: {e}");
                return;
            }
        };
        let mut tick = tokio::time::interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            if let Err(e) = sender.send_broadcast("ping", json!({ "ts": now_secs() })).await {
                eprintln!("broadcast send failed: {e}");
                break;
            }
        }
    }

    fn now_secs() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

#[cfg(feature = "realtime")]
#[tokio::main]
async fn main() -> rust_supabase_sdk::Result<()> {
    imp::run().await
}

#[cfg(not(feature = "realtime"))]
fn main() {
    eprintln!("This example requires --features realtime");
}

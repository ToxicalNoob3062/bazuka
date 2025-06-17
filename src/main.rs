use bazuka::{SkmvCache, SkmvConfig};
use tokio;

#[tokio::main]
async fn main() {
    // Create a cache with a maximum of 100 keys, 10 values per key, and 60s TTL.
    let cache = SkmvCache::<String, String>::new(SkmvConfig {
        maximum_capacity: 100,
        maximum_values_per_key: 10,
        idle_timeout: Some(60),
        time_to_live: Some(60),
    });

    // Insert values with per-value TTL (in seconds)
    cache.insert("user:1".to_string(), "session:abc".to_string(), 30).await;
    cache.insert("user:1".to_string(), "session:def".to_string(), 45).await;

    // Retrieve all values for a key
    let sessions = cache.get(&"user:1".to_string()).await;
    for session in sessions {
        println!("Active session: {}", session);
    }

    // Remove a specific value for a key
    cache.remove("user:1".to_string(), "session:abc".to_string()).await;
}
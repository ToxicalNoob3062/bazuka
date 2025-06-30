# Bazuka

Bazuka is a high-performance, asynchronous, multi-value cache for Rust for tokio runtime. It allows you to associate multiple values with a single key, each with its own expiry, and is designed for concurrent use in async environments (e.g., with Tokio).

## Features

- **Multi-value per key:** Store multiple values for each key.
- **Per-value TTL:** Each value can have its own time-to-live.
- **Async and thread-safe:** Built for use with Tokio and safe for concurrent access.
- **Customizable expiry:** Supports idle and timeout expiry for keys.
- **Efficient memory usage:** Backed by moka and dashmap for speed and safety.

## Usage

```rust
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
```

## API Overview

- `SkmvCache::new(config)`: Create a new cache with the given configuration.
- `insert(key, value, ttl)`: Insert a value for a key with a specific TTL (seconds) or update a previously inserted pair's ttl.
- `get(&key) -> Vec<Arc<V>>`: Get all values for a key.
- `remove(key, value)`: Remove a specific value for a key.

## Configuration

```rust
pub struct SkmvConfig {
    pub maximum_capacity: usize,        // Max number of keys
    pub maximum_values_per_key: usize,  // Max values per key
    pub idle_timeout: Option<u32>,      // Idle timeout in seconds
    pub time_to_live: Option<u32>,      // TTL in seconds
}
```

## Example: Per-value Expiry

```rust
cache.insert("k1".to_string(), "v1".to_string(), 5).await; // expires in 5s
cache.insert("k1".to_string(), "v2".to_string(), 10).await; // expires in 10s

// update ttl
cache.insert("k1".to_string(), "v1".to_string(), 8).await; // updated ttl of the specific value to 8 sec

// iter over the cache
let mut iter = cache.iter().await;
while let Some((key, value)) = iter.next() {
    println!("Key: {:?}, Value: {:?}", key, value);
}
```

## Testing

The crate includes comprehensive async tests for insertion, retrieval, removal, concurrency, and expiry.

## License

MIT

---

*Bazuka: Fast, flexible, async multi-value cache for Rust.*
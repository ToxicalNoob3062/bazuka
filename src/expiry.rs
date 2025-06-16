use moka::Expiry;
use std::sync::Arc;
use tokio::time::{Duration};
use std::time::Instant;
use std::hash::Hash;



pub struct DataExpiry;

impl<K, V> Expiry<Arc<(K,V)>,u32> for DataExpiry where 
    K: Hash + Eq + Send + Sync + 'static,
    V: Hash + Eq + Send + Sync + 'static,
  {
    fn expire_after_create(
        &self,
        _key: &Arc<(K, V)>,
        value: &u32,
        _created_at: Instant,
    ) -> Option<Duration> {
        println!("Setting expiry for value: {}", value);
        Some(Duration::from_secs(*value as u64))
    }

    fn expire_after_update(
        &self,
        _key: &Arc<(K, V)>,
        value: &u32,
        _updated_at: Instant,
        _current_duration: Option<Duration>,
    ) -> Option<Duration> {
        println!("Updating expiry for value: {}", value);
        Some(Duration::from_secs(*value as u64))
    }
}
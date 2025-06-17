use super::expiry::DataExpiry;
use dashmap::DashSet;
use moka::future::Cache as MokaCache;
use moka::notification::RemovalCause;
use std::hash::Hash;
use std::sync::Arc;
use tokio::sync::OnceCell;
use tokio::time::Duration;

type Tracker<K, V> = MokaCache<Arc<K>, Arc<DashSet<Arc<(Arc<K>, Arc<V>)>>>>;
type Cache<K, V> = MokaCache<Arc<(Arc<K>, Arc<V>)>, u32>;
type Vtable<K, V> = Arc<(OnceCell<Tracker<K, V>>, OnceCell<Cache<K, V>>)>;

pub struct SkmvConfig {
    pub maximum_capacity: usize,
    pub maximum_values_per_key: usize,
    pub idle_timeout: Option<Duration>,
    pub time_to_live: Option<Duration>,
}


#[derive(Debug)]
pub struct SkmvCache<K, V>
where
    K: Hash + Eq + Send + Sync + 'static,
    V: Hash + Eq + Send + Sync + 'static,
{
    vtable: Vtable<K, V>,
}

impl<K, V> SkmvCache<K, V>
where
    K: Hash + Eq + Send + Sync + 'static + std::fmt::Debug,
    V: Hash + Eq + Send + Sync + 'static + std::fmt::Debug,
{
    async fn tracker_eviction_listener(
        vtable: Vtable<K, V>,
        evicted_data: Arc<DashSet<Arc<(Arc<K>, Arc<V>)>>>,
        cause: RemovalCause,
    ) {
        if RemovalCause::Explicit != cause {
            if let Some(cache) = vtable.1.get() {
                for item in evicted_data.iter() {
                    cache.invalidate(item.key()).await;
                }
            }
        }
    }

    async fn cache_eviction_listener(
        vtable: Vtable<K, V>,
        evicted_data: &Arc<(Arc<K>, Arc<V>)>,
        cause: RemovalCause,
    ) {
        if RemovalCause::Explicit != cause {
            if let Some(tracker) = vtable.0.get() {
                if let Some(set) = tracker.get(&evicted_data.0).await {
                    set.remove(evicted_data);
                    if set.is_empty() {
                        tracker.invalidate(&evicted_data.0).await;
                    }
                }
            }
        }
    }

    /// Creates a new instance of `SkmvCache` with the provided `SkmvConfig` configuration.
    pub fn new(config: SkmvConfig) -> Self {
        let vtable = Arc::new((OnceCell::new(), OnceCell::new()));
        //creating tracker with eviction listener
        let vtable_clone = vtable.clone();
        let tracker = MokaCache::builder()
            .max_capacity(config.maximum_capacity as u64)
            .time_to_idle(config.idle_timeout.unwrap_or(Duration::from_secs(60)))
            .time_to_live(config.time_to_live.unwrap_or(Duration::from_secs(60)))
            .async_eviction_listener(move |_k, evicted_data, cause| {
                let vtable = Arc::clone(&vtable_clone);
                Box::pin(async move {
                    Self::tracker_eviction_listener(vtable, evicted_data, cause).await
                })
            })
            .build();

        //creating cache with expiry
        let vtable_clone = vtable.clone();
        let cache = MokaCache::builder()
            .max_capacity((config.maximum_capacity * config.maximum_values_per_key) as u64)
            .expire_after(DataExpiry)
            .async_eviction_listener(move |k, _v, cause| {
                let vtable = Arc::clone(&vtable_clone);
                Box::pin(async move { Self::cache_eviction_listener(vtable, &k, cause).await })
            })
            .build();

        vtable.0.set(tracker).unwrap();
        vtable.1.set(cache).unwrap();
        SkmvCache { vtable }
    }

    /// Inserts a value against a key if it does not already exist, or updates the value if it does where every value has its own expiry time.
    pub async fn insert(&self, key: K, value: V, ttl: u32) {
        let key = Arc::new(key);
        let key_tuple = Arc::new((key.clone(), Arc::new(value)));
        let tracker = self.vtable.0.get().unwrap();
        let cache = self.vtable.1.get().unwrap();
        if let Some(_) = cache.get(&key_tuple).await {
            cache.insert(key_tuple.clone(), ttl).await;
        } else {
            if let Some(set) = tracker.get(&key).await {
                set.insert(key_tuple.clone());
                cache.insert(key_tuple.clone(), ttl).await;
            } else {
                let set = DashSet::new();
                set.insert(key_tuple.clone());
                cache.insert(key_tuple.clone(), ttl).await;
                tracker.insert(key.clone(), Arc::new(set)).await;
            }
        }
    }

    /// Returns a vector of cloned values for the given key.
    pub async fn get(&self, key: K) -> Vec<Arc<V>> {
        let tracker = self.vtable.0.get().unwrap();
        let set = tracker.get(&key).await.unwrap_or_default();
        set.iter().map(|item| item.key().1.clone()).collect()
    }

    /// Removes a value against a key.
    pub async fn remove(&self, key: K, value: V) {
        let key = Arc::new(key);
        let key_tuple = Arc::new((key.clone(), Arc::new(value)));
        let tracker = self.vtable.0.get().unwrap();
        if let Some(set) = tracker.get(&key).await {
            set.remove(&key_tuple);
            if set.is_empty() {
                tracker.invalidate(&key).await;
            }
        }
        let cache = self.vtable.1.get().unwrap();
        cache.invalidate(&key_tuple).await;
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skmv_creation() {
        // test if you can create a new SkmvCache without a panic
        SkmvCache::<String, String>::new(
            SkmvConfig {
                maximum_capacity: 100,
                maximum_values_per_key: 10,
                idle_timeout: Some(Duration::from_secs(60)),
                time_to_live: Some(Duration::from_secs(60)),
            },
        );
    }

    #[tokio::test]
    async fn test_skmv_insert_and_get() {
        let cache = SkmvCache::<String, String>::new(
            SkmvConfig {
                maximum_capacity: 100,
                maximum_values_per_key: 10,
                idle_timeout: Some(Duration::from_secs(60)),
                time_to_live: Some(Duration::from_secs(60)),
            },
        );

        cache.insert("key1".to_string(), "value1".to_string(), 5).await;
        let values = cache.get("key1".to_string()).await;
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].as_str(), "value1");
    }

    #[tokio::test]
    async fn test_skmv_insert_and_remove() {
        let cache = SkmvCache::<String, String>::new(
            SkmvConfig {
                maximum_capacity: 100,
                maximum_values_per_key: 10,
                idle_timeout: Some(Duration::from_secs(60)),
                time_to_live: Some(Duration::from_secs(60)),
            },
        );

        cache.insert("key1".to_string(), "value1".to_string(), 5).await;
        cache.remove("key1".to_string(), "value1".to_string()).await;
        let values = cache.get("key1".to_string()).await;
        assert!(values.is_empty());
    }

    // test muitple insertions, retievals and removals
    #[tokio::test]
    async fn test_skmv_multiple_operations() {
        let cache = SkmvCache::<String, String>::new(
            SkmvConfig {
                maximum_capacity: 100,
                maximum_values_per_key: 10,
                idle_timeout: Some(Duration::from_secs(60)),
                time_to_live: Some(Duration::from_secs(60)),
            },
        );

        println!("Running multiple operations test...\n");

        cache.insert("key1".to_string(), "value1".to_string(), 5).await;
        cache.insert("key1".to_string(), "value2".to_string(), 5).await;
        let values = cache.get("key1".to_string()).await;
        assert_eq!(values.len(), 2);
        assert!(values.contains(&Arc::new("value1".to_string())));
        assert!(values.contains(&Arc::new("value2".to_string())));

        cache.remove("key1".to_string(), "value1".to_string()).await;
        let values = cache.get("key1".to_string()).await;
        assert_eq!(values.len(), 1);
        assert!(values.contains(&Arc::new("value2".to_string())));
    }
}
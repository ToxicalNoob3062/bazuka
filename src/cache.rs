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

/// Configuration for the SkmvCache initialization via `SkmvCache::new`.
pub struct SkmvConfig {
    /// Maximum number of keys in the cache.
    pub maximum_capacity: usize,
    /// Maximum number of values per key.
    pub maximum_values_per_key: usize,
    /// Idle timeout for each key in the cache. When key expires due to inactivity all associated values are also removed.
    ///
    /// Default is 60 seconds.
    pub idle_timeout: Option<u32>,
    /// Time to live for each key in the cache. When key expires due to timeout all associated values are also removed.
    ///
    /// Default is 60 seconds.
    pub time_to_live: Option<u32>,
}

/// `SkmvCache` is a specialized cache that allows storing multiple values against a single key, with each value having its own expiry time.
/// It is an tokio-based asynchronous cache that is designed to be cloned and shared across multiple threads by tokio tasks.
#[derive(Debug, Clone)]
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
        if RemovalCause::Explicit != cause && RemovalCause::Replaced != cause {
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
    ///
    /// # Example
    ///
    /// ```ignore
    /// use bazuka::{SkmvCache, SkmvConfig};
    /// let cache = SkmvCache::<String, String>::new(SkmvConfig {
    ///   maximum_capacity: 100,
    ///   maximum_values_per_key: 10,
    ///   idle_timeout: Some(30), 
    ///   time_to_live: Some(40),
    /// });
    /// ```
    pub fn new(config: SkmvConfig) -> Self {
        let vtable = Arc::new((OnceCell::new(), OnceCell::new()));
        //creating tracker with eviction listener
        let vtable_clone = vtable.clone();
        let tracker = MokaCache::builder()
            .max_capacity(config.maximum_capacity as u64)
            .time_to_idle(config.idle_timeout.map(|arg0: u32| Duration::from_secs(arg0 as u64)).unwrap_or(Duration::from_secs(60)))
            .time_to_live(config.time_to_live.map(|arg0: u32| Duration::from_secs(arg0 as u64)).unwrap_or(Duration::from_secs(60)))
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

    /// Inserts a value against a key if it does not already exist.
    /// 
    /// If the key value pair already exists, it updates the TTL for that value.
    /// 
    /// # Example
    ///
    /// ```ignore
    /// 
    /// //Insert a value against a key with TTL of 5 seconds
    /// cache.insert("key1".to_string(), "value1".to_string(), 5).await;
    /// cache.insert("key1".to_string(), "value2".to_string(), 5).await;
    /// 
    /// // Update the TTL for the value
    /// cache.insert("key1".to_string(), "value1".to_string(), 10).await;
    /// ```
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

    /// Returns a vector of cloned values for the given key. Copy if needed.
    /// 
    /// If the key has multiple values, it returns all of them.
    /// 
    /// # Example
    /// 
    /// ```ignore
    /// let values = cache.get(&"key1".to_string()).await;
    /// ```
    pub async fn get(&self, key: &K) -> Vec<Arc<V>> {
        let tracker = self.vtable.0.get().unwrap();
        let cache = self.vtable.1.get().unwrap();
        let keys_to_check: Vec<Arc<(Arc<K>, Arc<V>)>> = {
            let set = tracker.get(key).await.unwrap_or_default();
            set.iter().map(|item_ref| item_ref.key().clone()).collect()
        };
        let mut values = Vec::new();
        for cache_key_tuple in keys_to_check {
            //  cache get may trigger eviction listener thats why we formed keys_to_check
            if cache.get(&cache_key_tuple).await.is_some() {
                values.push(cache_key_tuple.1.clone());
            }
        }
        values
    }

    /// Removes a value against a key.
    /// 
    /// # Example
    /// ```ignore
    /// cache.remove("key1".to_string(), "value1".to_string()).await;
    /// ```
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

    /// Returns an iterator over all key-value pairs in the cache.
    /// The iterator yields tuples of `(Arc<K>, Arc<V>, u32)`, where `K` is the key type and `V` is the value type and `u32` is the TTL (time-to-live) for the value that was inserted (lifespan).
    /// (not the remaining TTL!!)
    /// 
    /// The iterator will not return any values that have been evicted or expired.
    /// # Example
    /// ```ignore
    /// let mut iter = cache.iter().await;
    /// while let Some((key, value, ttl)) = iter.next() {
    ///    println!("Key: {:?}, Value: {:?}, TTL: {:?}", key, value, ttl);
    /// }
    ///  ```
    pub async fn iter(&self) -> impl Iterator<Item = (Arc<K>, Arc<V>, u32)> + '_ {
        let cache = self.vtable.1.get().unwrap();
        cache.run_pending_tasks().await; 
        cache.iter().map(|(tuple_arc, ttl)| {
            let (k, v) = &**tuple_arc;
            (k.clone(), v.clone(), ttl)
        })
    }

    //force eviction
    #[allow(dead_code)]
    async fn force_eviction(&self) {
        let cache = self.vtable.1.get().unwrap();
        cache.run_pending_tasks().await;
    }
}

#[cfg(test)]
mod tests {
    use rand::Rng;

    use super::*;

    #[test]
    fn test_skmv_creation() {
        // test if you can create a new SkmvCache without a panic
        SkmvCache::<String, String>::new(SkmvConfig {
            maximum_capacity: 100,
            maximum_values_per_key: 10,
            idle_timeout: None,
            time_to_live: None,
        });
    }

    #[tokio::test]
    async fn test_skmv_insert_and_get() {
        let cache = SkmvCache::<String, String>::new(SkmvConfig {
            maximum_capacity: 100,
            maximum_values_per_key: 10,
            idle_timeout: None,
            time_to_live: None,
        });

        cache
            .insert("key1".to_string(), "value1".to_string(), 5)
            .await;
        let values = cache.get(&"key1".to_string()).await;
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].as_str(), "value1");
    }

    #[tokio::test]
    async fn test_skmv_iter() {
        let cache = SkmvCache::<String, String>::new(SkmvConfig {
            maximum_capacity: 100,
            maximum_values_per_key: 10,
            idle_timeout: None,
            time_to_live: None,
        });

        cache
            .insert("key1".to_string(), "value1".to_string(), 5)
            .await;
        cache
            .insert("key2".to_string(), "value2".to_string(), 5)
            .await;

        let mut iter = cache.iter().await;
        // iter.next has no guarantee of order, so we can only check if the values are present
        // check first 2 next are not None
        assert!(iter.next().is_some());
        assert!(iter.next().is_some());
        assert_eq!(iter.next(), None);
    }

    #[tokio::test]
    async fn test_skmv_insert_and_remove() {
        let cache = SkmvCache::<String, String>::new(SkmvConfig {
            maximum_capacity: 100,
            maximum_values_per_key: 10,
            idle_timeout: None,
            time_to_live: None,
        });

        cache
            .insert("key1".to_string(), "value1".to_string(), 5)
            .await;
        cache.remove("key1".to_string(), "value1".to_string()).await;
        let values = cache.get(&"key1".to_string()).await;
        assert!(values.is_empty());
    }

    // test muitple insertions, retievals and removals
    #[tokio::test]
    async fn test_skmv_multiple_operations() {
        let cache = SkmvCache::<String, String>::new(SkmvConfig {
            maximum_capacity: 100,
            maximum_values_per_key: 10,
            idle_timeout: None,
            time_to_live: None
        });

        println!("Running multiple operations test...\n");

        cache
            .insert("key1".to_string(), "value1".to_string(), 5)
            .await;
        cache
            .insert("key1".to_string(), "value2".to_string(), 5)
            .await;
        let values = cache.get(&"key1".to_string()).await;
        assert_eq!(values.len(), 2);
        assert!(values.contains(&Arc::new("value1".to_string())));
        assert!(values.contains(&Arc::new("value2".to_string())));

        cache.remove("key1".to_string(), "value1".to_string()).await;
        let values = cache.get(&"key1".to_string()).await;
        assert_eq!(values.len(), 1);
        assert!(values.contains(&Arc::new("value2".to_string())));
    }

    #[tokio::test]
    async fn test_skmv_concurrency() {
        // launch 100 task that insert k_i for v_i for 10 sec;
        // wait for 5 sec and then access a random key between 0 and 99
        let cache = SkmvCache::<String, String>::new(SkmvConfig {
            maximum_capacity: 100,
            maximum_values_per_key: 10,
            idle_timeout: None,
            time_to_live: None,
        });
        let mut handles = vec![];
        for i in 0..100 {
            let cache_clone = cache.clone();
            let key = format!("key_{}", i);
            let value = format!("value_{}", i);
            handles.push(tokio::spawn(async move {
                cache_clone.insert(key, value, 10).await;
            }));
        }
        for handle in handles {
            handle.await.unwrap();
        }
        // print insertion complete message
        println!("Insertion complete, now accessing random keys...\n");
        // assert the cache size is 100
        assert_eq!(cache.vtable.1.get().unwrap().iter().count(), 100);

        // access random keys from different tasks
        let mut handles = vec![];
        let mut rng = rand::rng();
        for _ in 0..100 {
            let cache_clone = cache.clone();
            let key_index = rng.random_range(0..100);
            let key = format!("key_{}", key_index);
            handles.push(tokio::spawn(async move {
                let values = cache_clone.get(&key).await;
                // assert to get values of length 1 and value should be "value_{key_index}"
                assert_eq!(values.len(), 1);
                assert_eq!(values[0].as_str(), &format!("value_{}", key_index));
            }));
        }
        for handle in handles {
            handle.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_skmv_rehydration() {
        // test if you can rehydrate the cache with the same config
        let cache = SkmvCache::<String, String>::new(SkmvConfig {
            maximum_capacity: 100,
            maximum_values_per_key: 10,
            idle_timeout: None,
            time_to_live: None,
        });

        cache
            .insert("key1".to_string(), "value1".to_string(), 3)
            .await;
        let values = cache.get(&"key1".to_string()).await;
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].as_str(), "value1");

        // rehydrate the cache by passing a different ttl with same key and value
        cache
            .insert("key1".to_string(), "value1".to_string(), 8)
            .await;

        // wait for 4 seconds to let the value expire
        tokio::time::sleep(Duration::from_secs(4)).await;

        // get the value again to check if it is still there as it has a new ttl for more 8 seconds if ttl was not updated it will be expiredd
        let values = cache.get(&"key1".to_string()).await;
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].as_str(), "value1");
        // print the cache for debugging
        println!("Cache after rehydration: {:?}", cache);
    }

    #[tokio::test]
    async fn test_skmv_eviction() {
        // test if you can evict the cache with the same config
        let cache = SkmvCache::<String, String>::new(SkmvConfig {
            maximum_capacity: 3,
            maximum_values_per_key: 10,
            idle_timeout: None,
            time_to_live: None,
        });

        cache
            .insert("key1".to_string(), "value1".to_string(), 2)
            .await;

        //wait for 3 seconds to let the value expire
        tokio::time::sleep(Duration::from_secs(5)).await;

        // get the value for implicit eviction
        let values = cache.get(&"key1".to_string()).await;
        println!("Values after implicit eviction: {:?}", values);

        // assert that the value is empty as it should have been evicted
        assert!(values.is_empty(), "Value should be evicted after ttl");
    }
}

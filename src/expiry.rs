use moka::Expiry;
use std::hash::Hash;
use std::sync::Arc;
use std::time::Instant;
use tokio::time::Duration;

pub struct DataExpiry;

impl<K, V> Expiry<Arc<(Arc<K>, Arc<V>)>, u32> for DataExpiry
where
    K: Hash + Eq + Send + Sync + 'static,
    V: Hash + Eq + Send + Sync + 'static,
{
    fn expire_after_create(
        &self,
        _key: &Arc<(Arc<K>, Arc<V>)>,
        value: &u32,
        _created_at: Instant,
    ) -> Option<Duration> {
        Some(Duration::from_secs(*value as u64))
    }

    fn expire_after_update(
        &self,
        _key: &Arc<(Arc<K>, Arc<V>)>,
        value: &u32,
        _updated_at: Instant,
        _current_duration: Option<Duration>,
    ) -> Option<Duration> {
        Some(Duration::from_secs(*value as u64))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    #[tokio::test]
    async fn test_key_hash() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let key = Arc::new((Arc::new("key1"), Arc::new("value1")));
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let key_hash = hasher.finish();

        let key2 = Arc::new((Arc::new("key1"), Arc::new("value2")));
        let mut hasher2 = DefaultHasher::new();
        key2.hash(&mut hasher2);
        let key2_hash = hasher2.finish();

        assert_ne!(
            key_hash, key2_hash,
            "Hashes should be different for different values under the same key"
        );
    }

    #[tokio::test]
    async fn test_key_hash_for_dset() {
        let dset = dashmap::DashSet::new();
        let key1 = Arc::new((Arc::new("key1"), Arc::new("value1")));
        let key2 = Arc::new((Arc::new("key1"), Arc::new("value2")));
        dset.insert(key1.clone());
        dset.insert(key2.clone());
        assert_eq!(dset.len(), 2, "DashSet should contain both keys with different values");
        assert!(dset.contains(&key1), "DashSet should contain key1");
        assert!(dset.contains(&key2), "DashSet should contain key2");
        // print the dset for debugging
        println!("Current DashSet: {:?}", dset);
    }
}
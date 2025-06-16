mod expiry;
mod cache;

mod test {
    use super::expiry::DataExpiry;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[tokio::test]
    async fn test_cache_size_eviction() {
        let cache = moka::future::Cache::builder()
            .max_capacity(3)
            .time_to_live(tokio::time::Duration::from_secs(5))
            .eviction_listener(|k: std::sync::Arc<&'static str>, _v, _cause| {
                // we get k2 here as k1,k3 and k4 has been accessed more frequently.
                println!("Evicting key: {}", k);
                assert_eq!(*k, "key2");
            })
            .build();
        cache.insert("key1", "value1").await;
        cache.insert("key2", "value2").await;
        cache.insert("key3", "value3").await;
        assert_eq!(cache.iter().count(), 3);
        // modify freq access pattern
        for _ in 0..100 {
            cache.get("key3").await;
        }
        for _ in 0..50 {
            cache.get("key1").await;
        }
        //insertion never triggers eviction
        cache.insert("key4", "value4").await;

        // lets increase k4 access frequency quickly before eviction cycle kicks in
        for i in 0..10 {
            cache.get("key4").await; // get will trigger eviction listener if ttl is reached
            print!("{},", i + 1);
        }
        println!("");
        //force eviction to happen
        cache.run_pending_tasks().await;
        assert_eq!(cache.iter().count(), 3);
    }

    #[tokio::test]
    async fn test_immediate_eviction() {
        let cache = moka::future::Cache::builder()
            .max_capacity(3)
            .time_to_live(tokio::time::Duration::from_secs(5))
            .eviction_listener(|k: std::sync::Arc<&'static str>, _v, _cause| {
                // we get k2 here as k1,k3 and k4 has been accessed more frequently.
                println!("Evicting key: {}", k);
            })
            .build();
        cache.insert("key1", "value1").await;
        cache.invalidate("key1").await;
        assert_eq!(cache.iter().count(), 0);
    }

    #[tokio::test]
    async fn test_cache_expiry() {
        let cache = moka::future::Cache::builder()
            .max_capacity(3)
            .expire_after(DataExpiry)
            .eviction_listener(|k, v, c| match c {
                moka::notification::RemovalCause::Expired => {
                    println!("Evicting expired key: {}, value: {}", k.0, v)
                }
                _ => println!("Evicting key: {}, value: {}, cause: {:?}", k.0, v, c),
            })
            .build();

        type Key<K, V> = Arc<(Arc<K>, Arc<V>)>;
        let k1 = Key::from((Arc::new("key1").to_owned(), Arc::new("value1").to_owned()));
        cache.insert(k1.clone(), 2).await;
        cache.insert(Key::from((Arc::new("key2").to_owned(), Arc::new("value2").to_owned())), 2).await;
        cache.insert(Key::from((Arc::new("key3").to_owned(), Arc::new("value3").to_owned())), 2).await;
        assert_eq!(cache.iter().count(), 3);

        // Update the expiry time for one item to be 5 seconds
        cache.insert(k1.clone(), 5).await;

        // Wait for the items to expire
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

        //force eviction to happen
        println!("Running pending tasks to trigger expiry");
        cache.run_pending_tasks().await;

        assert_eq!(cache.iter().count(), 1);
    }

    #[tokio::test]
    async fn test_eviction_laziness() {
        let flag = Arc::new(AtomicBool::new(false));
        let cache = moka::future::Cache::builder()
            .max_capacity(3)
            .time_to_live(tokio::time::Duration::from_secs(2))
            .eviction_listener({
                let flag = Arc::clone(&flag);
                move |_k: std::sync::Arc<&'static str>, _v: u32, _cause| {
                    println!("Evicting key: {}", _k);
                    flag.store(true, Ordering::Relaxed);
                }
            })
            .build();

        cache.insert("key1", 2).await;
        tokio::time::sleep(tokio::time::Duration::from_secs(4)).await;

        // expecting eviction handler not to be called and flag to be false
        assert!(!flag.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_eviction_laziness_2() {
        let flag = Arc::new(AtomicBool::new(false));
        let cache = moka::future::Cache::builder()
            .max_capacity(1)
            .time_to_live(tokio::time::Duration::from_secs(5))
            .eviction_listener({
                let flag = Arc::clone(&flag);
                move |_k: std::sync::Arc<&'static str>, _v: u32, _cause| {
                    println!("Evicting key: {}", _k);
                    flag.store(true, Ordering::Relaxed);
                }
            })
            .build();

        cache.insert("key1", 2).await;
        cache.insert("key2", 2).await;
        cache.insert("key3", 2).await;

        // expecting eviction handler not to be called and flag to be false even overflowing capcity 
        // as it's waiting for eviction cycle
        assert!(!flag.load(Ordering::Relaxed));
    }
}

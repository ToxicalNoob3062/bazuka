mod test {
    #[tokio::test]
    async fn test_cache_size_eviction() {
        let cache = moka::future::Cache::builder()
            .max_capacity(3)
            .time_to_live(tokio::time::Duration::from_secs(5))
            .eviction_listener(|k:std::sync::Arc<&'static str>, _v, _cause| {
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
        //listener is not triggered as soon as k4 is inserted it waits for the next eviction batch
        cache.insert("key4", "value4").await;

        // lets increase k4 access frequency quickly before eviction happens as get also doesn't trigger eviction
        for i in 0..10 {
            cache.get("key4").await;
            print!("{},", i+1);
        }
        println!("");
        //force eviction to happen
        cache.run_pending_tasks().await;
        assert_eq!(cache.iter().count(), 3);
    }
}

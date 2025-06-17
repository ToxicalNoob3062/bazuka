use std::{sync::Arc, time::Duration};

use bazuka::{
    SkmvCache,
    SkmvConfig,
};


#[tokio::main]
async fn main() {
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
        // print all values
        println!("Values for key1: {:?}", values);
        assert_eq!(values.len(), 2);
        assert!(values.contains(&Arc::new("value1".to_string())));
        assert!(values.contains(&Arc::new("value2".to_string())));

        cache.remove("key1".to_string(), "value1".to_string()).await;
        let values = cache.get("key1".to_string()).await;
        assert_eq!(values.len(), 1);
        assert!(values.contains(&Arc::new("value2".to_string())));

         // print the cache
        println!("Current Cache: {:?}", cache);
}

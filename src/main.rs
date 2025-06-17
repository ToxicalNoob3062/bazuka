use bazuka::{SkmvCache, SkmvConfig};

#[tokio::main]
async fn main() {
    let cache = SkmvCache::<String, String>::new(SkmvConfig {
        maximum_capacity: 100,
        maximum_values_per_key: 10,
        idle_timeout: Some(30), 
        time_to_live: Some(30), 
    });

    // do insert 100 values from async tasks
    let mut handles = vec![];
    for i in 0..100 {
        let cache_clone = cache.clone();
        let handle = tokio::spawn(async move {
            cache_clone.insert(format!("key{}", i), format!("value{}", i), i + 5).await;
        });
        handles.push(handle);
    }
    for handle in handles {
        handle.await.unwrap();
    }

    // do get 100 values from async tasks
    let mut handles = vec![];
    for i in 0..100 {
        let cache_clone = cache.clone();
        let handle = tokio::spawn(async move {
            println!("Got: {:?}", cache_clone.get(&format!("key{}", i)).await);
        });
        handles.push(handle);
    }
    for handle in handles {
        handle.await.unwrap();
    }

    // add 2 more value to key 5
    cache.insert("key5".to_string(), "value105".to_string(), 10).await;
    cache.insert("key5".to_string(), "value106".to_string(), 10).await;

    // update the ttl for val106
    cache.insert("key5".to_string(), "value106".to_string(), 20).await;
}

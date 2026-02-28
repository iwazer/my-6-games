use redis::aio::ConnectionManager;
use redis::Client;

pub async fn create_connection_manager(redis_url: &str) -> ConnectionManager {
    let client = Client::open(redis_url).expect("Invalid Redis URL");
    ConnectionManager::new(client)
        .await
        .expect("Failed to connect to Redis")
}

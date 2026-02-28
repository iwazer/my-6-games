use sqlx::MySqlPool;

pub async fn create_pool(database_url: &str) -> MySqlPool {
    MySqlPool::connect(database_url)
        .await
        .expect("Failed to connect to MariaDB")
}

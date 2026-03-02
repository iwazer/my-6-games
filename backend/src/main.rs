#[macro_use]
extern crate rocket;

mod cache;
mod config;
mod db;
mod models;
mod routes;
mod services;

use rocket_dyn_templates::Template;
use services::igdb::IgdbClient;
use services::image::ImageService;

#[launch]
async fn rocket() -> _ {
    dotenvy::dotenv().ok();

    let cfg = config::AppConfig::from_env();
    let db_pool = db::create_pool(&cfg.database_url).await;
    let redis_conn = cache::create_connection_manager(&cfg.redis_url).await;
    let igdb = IgdbClient::new(&cfg, redis_conn.clone());
    let image_svc = ImageService::new();

    // 起動時に期限切れの共有データを削除
    sqlx::query("DELETE FROM shares WHERE expires_at < NOW()")
        .execute(&db_pool)
        .await
        .ok();

    rocket::build()
        .manage(cfg)
        .manage(db_pool)
        .manage(redis_conn)
        .manage(igdb)
        .manage(image_svc)
        .attach(Template::fairing())
        .mount(
            "/",
            routes![
                routes::health::health,
                routes::pages::index,
                routes::pages::share_page,
            ],
        )
        .mount(
            "/api",
            routes![
                routes::games::search_games,
                routes::shares::create_share,
                routes::shares::get_share,
                routes::shares::share_image,
                routes::shares::share_image_ogp,
            ],
        )
}

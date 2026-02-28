#[macro_use]
extern crate rocket;

mod cache;
mod config;
mod db;
mod routes;

use rocket_dyn_templates::Template;

#[launch]
async fn rocket() -> _ {
    dotenvy::dotenv().ok();

    let cfg = config::AppConfig::from_env();
    let db_pool = db::create_pool(&cfg.database_url).await;
    let redis_conn = cache::create_connection_manager(&cfg.redis_url).await;

    rocket::build()
        .manage(cfg)
        .manage(db_pool)
        .manage(redis_conn)
        .attach(Template::fairing())
        .mount("/", routes![routes::health::health])
}

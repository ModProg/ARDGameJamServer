use actix_web::web::Data;
use actix_web::{App, HttpServer};
use anyhow::Result;

mod db;
use db::DB;

mod web;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("warn"));

    let database = Data::new(DB::new().await?);

    HttpServer::new(move || App::new().app_data(database.clone()).service(web::scope()))
        .bind("0.0.0.0:8000")?
        .run()
        .await?;

    Ok(())
}

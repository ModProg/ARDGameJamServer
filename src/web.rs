use actix_web::dev::HttpServiceFactory;
use actix_web::error::ErrorInternalServerError;
use actix_web::web::{Data, Json, Path};
use actix_web::{get, post, routes};
use serde::Serialize;
use time::OffsetDateTime;

use crate::db::{DB, Highscore};

#[derive(Serialize)]
struct Container {
    list: Vec<Highscore>,
}

pub fn scope() -> impl HttpServiceFactory {
    (get, get_around, post, delete)
}

#[routes]
#[get("/{user}/delete")]
#[delete("/{user}")]
async fn delete(db: Data<DB>, user: Path<String>) -> actix_web::Result<()> {
    db.delete(user.into_inner())
        .await
        .map_err(ErrorInternalServerError)
}

#[get("/")]
async fn get(db: Data<DB>) -> actix_web::Result<Json<Container>> {
    let list = (**db).get_top().await.map_err(ErrorInternalServerError)?;
    Ok(Json(Container { list }))
}

#[get("/around/{user}")]
async fn get_around(db: Data<DB>, user: Path<String>) -> actix_web::Result<Json<Container>> {
    let list = (**db)
        .get_around(user.into_inner())
        .await
        .map_err(ErrorInternalServerError)?;
    Ok(Json(Container { list }))
}

#[post("/")]
async fn post(db: Data<DB>, Json(mut highscore): Json<Highscore>) -> actix_web::Result<()> {
    highscore.created_at = OffsetDateTime::now_utc();
    db.insert(highscore)
        .await
        .map_err(ErrorInternalServerError)?;
    Ok(())
}

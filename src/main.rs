mod database;
mod fts_tree;
mod model;

use actix_identity::{CookieIdentityPolicy, Identity, IdentityService};
use actix_web::{error, web, App, HttpResponse, HttpServer};
use database::*;
use model::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

type Tera = web::Data<tera::Tera>;
type Db = web::Data<sled::Db>;

async fn index(id: Identity, tera: Tera, db: Db) -> actix_web::Result<HttpResponse> {
    let mut ctx = tera::Context::new();
    if let Some(username) = id.identity() {
        let (_user_id, user) = db
            .get_user_by_username(&username)
            .map_err(|_| error::ErrorInternalServerError("Database error"))?
            .ok_or(error::ErrorInternalServerError("Authentication error"))?;
        ctx.insert("user", &user);
    }
    let body = tera
        .render("index.html", &ctx)
        .map_err(|_| error::ErrorInternalServerError("Template error"))?;
    Ok(HttpResponse::Ok().body(body))
}

async fn login(tera: Tera) -> actix_web::Result<HttpResponse> {
    let ctx = tera::Context::new();
    let body = tera
        .render("login.html", &ctx)
        .map_err(|_| error::ErrorInternalServerError("Template error"))?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

#[derive(Serialize, Deserialize)]
struct LoginParams {
    username: String,
    password: String,
}

async fn login_post(
    params: web::Form<LoginParams>,
    id: Identity,
    db: Db,
) -> actix_web::Result<HttpResponse> {
    if let Some((_user_id, user)) = db
        .get_user_by_username(&params.username)
        .map_err(|_| error::ErrorInternalServerError("Database error"))?
    {
        if bcrypt::verify(&params.password, &user.password_hash)
            .map_err(|_| error::ErrorInternalServerError("Verification error"))?
        {
            id.remember(user.username);
            return Ok(HttpResponse::Found().header("location", "/").finish());
        }
    }
    Ok(HttpResponse::Found()
        .header("location", "/login?wrong_password")
        .finish())
}

async fn logout(id: Identity) -> actix_web::Result<HttpResponse> {
    id.forget();
    Ok(HttpResponse::Found()
        .header("location", "/login?logout")
        .finish())
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    let private_key = [0u8; 32];
    HttpServer::new(move || {
        let tera = tera::Tera::new(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/**/*")).unwrap();
        let db = sled::Config::new().temporary(true).open().unwrap();
        let _pulp_fiction_id = db
            .add_movie(&Movie {
                name: "Pulp Fiction".to_owned(),
            })
            .unwrap()
            .unwrap();
        let _admin_id = db
            .add_user(&User {
                username: "admin".to_owned(),
                password_hash: bcrypt::hash("password", bcrypt::DEFAULT_COST).unwrap(),
                friends: HashMap::new(),
            })
            .unwrap()
            .unwrap();
        App::new()
            .wrap(IdentityService::new(
                CookieIdentityPolicy::new(&private_key)
                    .name("auth-cookie")
                    .secure(false),
            ))
            .data(tera)
            .data(db)
            .route("/", web::get().to(index))
            .route("/login", web::get().to(login))
            .route("/login", web::post().to(login_post))
            .route("/logout", web::get().to(logout))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}

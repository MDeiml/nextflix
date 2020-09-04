mod database;
mod fts_tree;
mod model;

use actix_identity::{CookieIdentityPolicy, Identity, IdentityService};
use actix_web::{error, middleware::Logger, web, App, HttpResponse, HttpServer};
use database::*;
use log::debug;
use model::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

type Tera = web::Data<tera::Tera>;
type Db = web::Data<sled::Db>;

fn log_error<E: std::fmt::Debug>(err: E, message: &'static str) -> error::Error {
    debug!("{:?}", err);
    error::ErrorInternalServerError(message)
}

async fn index(id: Identity, tera: Tera, db: Db) -> actix_web::Result<HttpResponse> {
    let mut ctx = tera::Context::new();
    if let Some(username) = id.identity() {
        let (_user_id, user) = db
            .get_user_by_username(&username)
            .map_err(|err| log_error(err, "Database error"))?
            .ok_or_else(|| {
                log_error(
                    format!("User does not exist: {}", username),
                    "Authentication error",
                )
            })?;
        ctx.insert("user", &user);
        let movie_ids = user
            .friends
            .values()
            .flat_map(|friend_data| &friend_data.movies)
            .collect::<HashSet<_>>();
        let movies = movie_ids
            .iter()
            .map(|movie_id| -> sled::Result<(String, Movie)> {
                Ok((movie_id.to_string(), db.get_movie(**movie_id)?.unwrap()))
            })
            .collect::<sled::Result<HashMap<_, _>>>()
            .map_err(|err| log_error(err, "Database error"))?;
        log::info!("movies: {:?}", &movies);
        ctx.insert("movies", &movies);
    }
    let body = tera
        .render("index.html", &ctx)
        .map_err(|err| log_error(err, "Template error"))?;
    Ok(HttpResponse::Ok().body(body))
}

async fn login(tera: Tera) -> actix_web::Result<HttpResponse> {
    let ctx = tera::Context::new();
    let body = tera
        .render("login.html", &ctx)
        .map_err(|err| log_error(err, "Template error"))?;
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
        .map_err(|err| log_error(err, "Database error"))?
    {
        if bcrypt::verify(&params.password, &user.password_hash)
            .map_err(|err| log_error(err, "Verification error"))?
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

    std::env::set_var("RUST_LOG", "nextflix=debug,actix_web=info");
    std::env::set_var("RUST_BACKTRACE", "1");
    env_logger::init();

    HttpServer::new(move || {
        let tera = tera::Tera::new(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/**/*")).unwrap();
        let db = sled::Config::new().temporary(true).open().unwrap();
        let pulp_fiction_id = db
            .add_movie(&Movie {
                name: "Pulp Fiction".to_owned(),
            })
            .unwrap()
            .unwrap();
        let admin_id = db
            .add_user(&User {
                username: "admin".to_owned(),
                password_hash: bcrypt::hash("password", bcrypt::DEFAULT_COST).unwrap(),
                friends: HashMap::new(),
            })
            .unwrap()
            .unwrap();
        let _foo_id = db
            .add_user(&User {
                username: "foo".to_owned(),
                password_hash: bcrypt::hash("1234", bcrypt::DEFAULT_COST).unwrap(),
                friends: vec![(
                    admin_id,
                    FriendData {
                        movies: vec![pulp_fiction_id],
                    },
                )]
                .into_iter()
                .collect(),
            })
            .unwrap()
            .unwrap();
        App::new()
            .wrap(Logger::default())
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

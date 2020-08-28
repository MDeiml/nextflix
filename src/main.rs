mod fts_tree;

use actix_identity::{CookieIdentityPolicy, Identity, IdentityService};
use actix_web::{error, web, App, HttpResponse, HttpServer};
use serde::{Deserialize, Serialize};

type Tera = web::Data<tera::Tera>;

async fn index(id: Identity, tera: Tera) -> actix_web::Result<HttpResponse> {
    let mut ctx = tera::Context::new();
    ctx.insert(
        "username",
        &id.identity().unwrap_or_else(|| "world".to_owned()),
    );
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
) -> actix_web::Result<HttpResponse> {
    // TODO: Check login data
    id.remember(params.username.to_owned());
    Ok(HttpResponse::Found().header("location", "/").finish())
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    let private_key = [0u8; 32];
    HttpServer::new(move || {
        let tera = tera::Tera::new(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/**/*")).unwrap();
        App::new()
            .wrap(IdentityService::new(
                CookieIdentityPolicy::new(&private_key)
                    .name("auth-cookie")
                    .secure(false),
            ))
            .data(tera)
            .route("/", web::get().to(index))
            .route("/login", web::get().to(login))
            .route("/login", web::post().to(login_post))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}

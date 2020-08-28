mod fts_tree;

use actix_web::{error, web, App, Error, HttpResponse, HttpServer, Responder};

async fn index() -> impl Responder {
    HttpResponse::Ok().body("Hello world")
}

async fn login(tera: web::Data<tera::Tera>) -> Result<HttpResponse, Error> {
    let ctx = tera::Context::new();
    let body = tera
        .render("login.html", &ctx)
        .map_err(|_| error::ErrorInternalServerError("Template error"))?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        let tera = tera::Tera::new(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/**/*")).unwrap();
        App::new()
            .data(tera)
            .route("/", web::get().to(index))
            .route("/login", web::get().to(login))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}

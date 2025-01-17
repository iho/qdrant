#[macro_use]
extern crate log;

mod api;
mod common;
mod settings;

use actix_web::middleware::Logger;
use actix_web::web::Data;
use actix_web::{error, get, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use serde::{Deserialize, Serialize};
use storage::content_manager::toc::TableOfContent;

use crate::api::collections_api::{get_collection, get_collections, update_collections};
use crate::api::recommend_api::recommend_points;
use crate::api::retrieve_api::{get_point, get_points, scroll_points};
use crate::api::search_api::search_points;
use crate::api::update_api::update_points;
use crate::common::helpers::create_search_runtime;
use crate::settings::Settings;

#[derive(Serialize, Deserialize)]
pub struct VersionInfo {
    pub title: String,
    pub version: String,
}

fn json_error_handler(err: error::JsonPayloadError, _req: &HttpRequest) -> error::Error {
    use actix_web::error::JsonPayloadError;

    let detail = err.to_string();
    let resp = match &err {
        JsonPayloadError::ContentType => HttpResponse::UnsupportedMediaType().body(detail),
        JsonPayloadError::Deserialize(json_err) if json_err.is_data() => {
            HttpResponse::UnprocessableEntity().body(detail)
        }
        _ => HttpResponse::BadRequest().body(detail),
    };
    error::InternalError::from_response(err, resp).into()
}

#[get("/")]
pub async fn index() -> impl Responder {
    HttpResponse::Ok().json(VersionInfo {
        title: "qdrant - vector search engine".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

fn main() -> std::io::Result<()> {
    let settings = Settings::new().expect("Can't read config.");
    std::env::set_var("RUST_LOG", &settings.log_level);
    env_logger::init();

    // Create and own search runtime out of the scope of async context to ensure correct
    // destruction of it
    let runtime = create_search_runtime(settings.storage.performance.max_search_threads)
        .expect("Can't create runtime.");
    let handle = runtime.handle().clone();

    actix_web::rt::System::new().block_on(async {
        let toc = TableOfContent::new(&settings.storage, handle);

        for collection in toc.all_collections() {
            info!("loaded collection: {}", collection);
        }

        let toc_data = web::Data::new(toc);

        HttpServer::new(move || {
            App::new()
                .wrap(Logger::default())
                .app_data(toc_data.clone())
                .app_data(Data::new(
                    web::JsonConfig::default()
                        .limit(32 * 1024 * 1024)
                        .error_handler(json_error_handler),
                )) // 32 Mb
                .service(index)
                .service(get_collections)
                .service(update_collections)
                .service(get_collection)
                .service(update_points)
                .service(get_point)
                .service(get_points)
                .service(scroll_points)
                .service(search_points)
                .service(recommend_points)
        })
        // .workers(4)
        .bind(format!(
            "{}:{}",
            settings.service.host, settings.service.port
        ))?
        .run()
        .await
    })
}

use actix_web::{
    error::ResponseError, http::StatusCode, routes, web, App, HttpResponse, HttpServer, Responder,
};
use clap::Parser;
use derive_more::{Display, Error};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::{
    select,
    sync::RwLock,
    time::{interval, Duration, Instant},
};
use tokio_stream::wrappers::IntervalStream;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    port: Option<u16>,
}

#[derive(Debug, Display, Error)]
enum PageError {
    #[display(fmt = "Couldn't find Page")]
    NotFound,
}

impl ResponseError for PageError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
        }
    }
}

type AppState = Arc<RwLock<HashMap<String, RwLock<RadioState>>>>;

#[routes]
#[get("/")]
#[get("/index.html")]
async fn start_page() -> impl Responder {
    HttpResponse::Ok().body("Start")
}

#[routes]
#[get("/auth")]
#[get("/auth/")]
async fn auth_page() -> impl Responder {
    HttpResponse::Ok()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RadioState {
    title: String,
    description: String,
}

#[routes]
#[get("/{radio}")]
#[get("/{radio}/")]
#[get("/{radio}/index.html")]
async fn radio_page(
    path: web::Path<String>,
    state: web::Data<AppState>,
) -> Result<HttpResponse, PageError> {
    let id = path.into_inner();
    let RadioState { title, description } = state
        .read()
        .await
        .get(&id)
        .ok_or(PageError::NotFound)?
        .read()
        .await
        .clone();
    Ok(HttpResponse::Ok().body(format!("Radio {title} ({id})\n {description}")))
}

#[routes]
#[get("/{radio}/edit")]
#[get("/{radio}/edit/")]
#[get("/{radio}/edit/index.html")]
async fn radio_edit(path: web::Path<String>) -> impl Responder {
    let id = path.into_inner();
    HttpResponse::Ok().body(format!("Edit {id}"))
}

#[derive(Deserialize, Serialize)]
struct Config {
    title: String,
}

#[routes]
#[post("/{radio}")]
#[post("/{radio}/")]
async fn radio_config(
    path: web::Path<String>,
    web::Json(new_state): web::Json<RadioState>,
    state: web::Data<AppState>,
) -> impl Responder {
    let id = path.into_inner();
    state
        .write()
        .await
        .insert(id.clone(), RwLock::new(new_state.clone()));
    HttpResponse::Ok().body(format!("Edited {id} with {}", new_state.title))
}

async fn update_hls(_instant: Instant, _data: AppState) {
    // TODO: Update the HLS data on to instant
    println!("{}µs", _instant.elapsed().as_micros())
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();
    let port = args.port.unwrap_or(8080);
    let data: AppState = Arc::new(RwLock::new(HashMap::new()));
    data.write().await.insert(
        "test".to_owned(),
        RwLock::new(RadioState {
            title: "Test".to_owned(),
            description: "This is a test station, \n ignore".to_owned(),
        }),
    );
    let sdata = data.clone();

    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(sdata.clone()))
            .service(start_page)
            .service(auth_page)
            .service(radio_page)
            .service(radio_edit)
    })
    .bind(("0.0.0.0", port))?
    .run();
    let hdata = data.clone();
    let hls = tokio::task::spawn(
        IntervalStream::new(interval(Duration::from_secs(10)))
            .then(move |instant| update_hls(instant, hdata.clone()))
            .collect::<()>(),
    );
    // NOTE: Only use Futures that only finish on unrecoverable errors (but we still want to exit gracefully)
    select! {
        x = server => return x,
        _ = hls => unreachable!()
    }
}

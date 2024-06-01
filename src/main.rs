use actix_web::{
    error::ResponseError, http::StatusCode, routes, web, App, HttpResponse, HttpServer, Responder,
};
use clap::Parser;
use derive_more::{Display, Error};
use futures::{future::join_all, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{mpsc::channel, Arc},
};
use tokio::{
    fs::read_to_string,
    select,
    sync::{mpsc::unbounded_channel, RwLock},
    time::{Duration, Instant},
};
use tokio_stream::wrappers::UnboundedReceiverStream;

mod blocking;
use blocking::ToBlocking;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    port: Option<u16>,
    pages: Option<PathBuf>,
}

/// Errors our webpages can return
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

/// Global async app state
struct AppState {
    pages: (&'static str, &'static str, &'static str),
    to_blocking: std::sync::mpsc::Sender<ToBlocking>,
    radio_states: RwLock<HashMap<String, RwLock<RadioState>>>,
}

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

/// Data for the radios
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
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse, PageError> {
    let id = path.into_inner();
    let RadioState { title, description } = state
        .radio_states
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

/// Radio Config sent by the frontend
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
    state: web::Data<Arc<AppState>>,
) -> impl Responder {
    let id = path.into_inner();
    state
        .radio_states
        .write()
        .await
        .insert(id.clone(), RwLock::new(new_state.clone()));
    HttpResponse::Ok().body(format!("Edited {id} with {}", new_state.title))
}

/// Function to add the new segments and set the new current segment
async fn update_hls(_instant: Instant, _data: Arc<AppState>) {
    // TODO: Update the HLS data on to instant
    println!("{}µs", _instant.elapsed().as_micros())
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();
    let port = args.port.unwrap_or(8080);
    let pages = if let Some(path) = args.pages {
        let mut start_path = path.clone();
        let mut radio_path = path.clone();
        let mut edit_path = path.clone();
        start_path.push("start.html");
        radio_path.push("radio.html");
        edit_path.push("edit.html");
        let files = join_all([
            read_to_string(start_path),
            read_to_string(radio_path),
            read_to_string(edit_path),
        ])
        .await
        .into_iter()
        .map(|s| s.map(|s| s.leak()))
        .collect::<Result<Box<_>, _>>()?;
        (&*files[0], &*files[1], &*files[2])
    } else {
        (
            include_str!("../resources/start.html"),
            include_str!("../resources/radio.html"),
            include_str!("../resources/edit.html"),
        )
    };
    let (atx, arx) = unbounded_channel();
    let (stx, srx) = channel();
    let data: Arc<AppState> = Arc::new(AppState {
        pages,
        to_blocking: stx,
        radio_states: RwLock::new(HashMap::new()),
    });
    data.radio_states.write().await.insert(
        "test".to_owned(),
        RwLock::new(RadioState {
            title: "Test".to_owned(),
            description: "This is a test station, \n ignore".to_owned(),
        }),
    );

    std::thread::spawn(|| blocking::main(atx, srx, Duration::from_secs(10)));

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
        UnboundedReceiverStream::new(arx)
            .then(move |instant| update_hls(instant, hdata.clone()))
            .collect::<()>(),
    );
    // NOTE: Only use Futures that only finish on unrecoverable errors (but we still want to exit gracefully)
    select! {
        x = server => return x,
        _ = hls => unreachable!()
    }
}

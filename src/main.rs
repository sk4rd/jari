use actix_web::{
    error::ResponseError, http::StatusCode, routes, web, App, HttpResponse, HttpServer, Responder,
};
use clap::Parser;
use derive_more::{Display, Error};
use futures::{channel::mpsc::TryRecvError, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{mpsc::channel, Arc},
};
use tokio::{
    select,
    sync::{mpsc::unbounded_channel, RwLock},
    time::{interval, Duration, Instant},
};
use tokio_stream::wrappers::{ReceiverStream, UnboundedReceiverStream};

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

type AppState = Arc<
    RwLock<(
        HashMap<String, RwLock<RadioState>>,
        std::sync::mpsc::Sender<ToBlocking>,
    )>,
>;

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
        .0
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
        .0
        .insert(id.clone(), RwLock::new(new_state.clone()));
    HttpResponse::Ok().body(format!("Edited {id} with {}", new_state.title))
}

async fn update_hls(_instant: Instant, _data: AppState) {
    // TODO: Update the HLS data on to instant
    println!("{}µs", _instant.elapsed().as_micros())
}

enum ToBlocking {}

fn blocking_main(
    _atx: tokio::sync::mpsc::UnboundedSender<Instant>,
    srx: std::sync::mpsc::Receiver<ToBlocking>,
    interval: Duration,
) {
    let mut last = std::time::Instant::now();
    loop {
        match srx.try_recv() {
            Ok(msg) => match msg {},
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
        }
        let diff = last.elapsed();
        if diff > interval {
            // TODO: send/create next fragment
            last += interval;
            _atx.send(last.clone().into()).unwrap();
        }
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();
    let port = args.port.unwrap_or(8080);
    let (atx, arx) = unbounded_channel();
    let (stx, srx) = channel();
    let data: AppState = Arc::new(RwLock::new((HashMap::new(), stx)));
    data.write().await.0.insert(
        "test".to_owned(),
        RwLock::new(RadioState {
            title: "Test".to_owned(),
            description: "This is a test station, \n ignore".to_owned(),
        }),
    );

    std::thread::spawn(|| blocking_main(atx, srx, Duration::from_secs(10)));

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

use actix_files::Files;
use actix_web::{
    error::ResponseError, get, http::StatusCode, routes, web, App, HttpResponse, HttpServer,
    Responder,
};
use clap::Parser;
use derive_more::{Display, Error};
use futures::{future::join_all, StreamExt};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::{
    fs::read_to_string,
    select,
    sync::{mpsc::unbounded_channel, RwLock},
    time::Duration,
};
use tokio_stream::wrappers::UnboundedReceiverStream;

mod blocking;
use blocking::ToBlocking;

mod hls;

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

const NUM_BANDWIDTHS: usize = 1;
const NUM_SEGMENTS: usize = 1;

const BANDWIDTHS: [usize; NUM_BANDWIDTHS] = [22000];
/// Radio Config sent by the frontend
#[derive(Debug, Clone, Deserialize, Serialize)]
struct Config {
    title: String,
    description: String,
}
/// Data for the radios
#[derive(Debug, Clone)]
struct RadioState {
    config: Config,
    playlist: hls::MasterPlaylist<NUM_BANDWIDTHS, NUM_SEGMENTS>,
}
/// Global async app state
struct AppState {
    pages: (&'static str, &'static str, &'static str),
    to_blocking: tokio::sync::mpsc::UnboundedSender<ToBlocking>,
    radio_states: RwLock<HashMap<String, RwLock<RadioState>>>,
}

#[routes]
#[get("/")]
#[get("/index.html")]
async fn start_page(state: web::Data<Arc<AppState>>) -> impl Responder {
    HttpResponse::Ok().body(state.pages.0)
}

#[routes]
#[get("/auth")]
#[get("/auth/")]
async fn auth_page() -> impl Responder {
    HttpResponse::Ok()
}

#[routes]
#[get("/{radio}")]
#[get("/{radio}/")]
#[get("/{radio}/index.html")]
async fn radio_page(
    path: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse, PageError> {
    let name = path.into_inner();
    // Extract Radio State
    let Config { title, description } = state
        .radio_states
        .read()
        .await
        .get(&name)
        .ok_or(PageError::NotFound)?
        .read()
        .await
        .config
        .clone();
    // Return formatted data
    Ok(HttpResponse::Ok().body(
        state
            .pages
            .1
            .replace("{title}", &title)
            .replace("{name}", &name)
            .replace("{description}", &description),
    ))
}

#[routes]
#[get("/{radio}/edit")]
#[get("/{radio}/edit/")]
#[get("/{radio}/edit/index.html")]
async fn radio_edit(
    path: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse, PageError> {
    let id = path.into_inner();
    let Config { title, description } = state
        .radio_states
        .read()
        .await
        .get(&id)
        .ok_or(PageError::NotFound)?
        .read()
        .await
        .config
        .clone();
    Ok(HttpResponse::Ok().body(
        state
            .pages
            .2
            .replace("{title}", &title)
            .replace("{id}", &id)
            .replace("{description}", &description),
    ))
}

#[routes]
#[post("/{radio}")]
#[post("/{radio}/")]
async fn radio_config(
    path: web::Path<String>,
    web::Json(config): web::Json<Config>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse, PageError> {
    let id = path.into_inner();
    // Change or add Radio by inserting into HashMap
    state
        .radio_states
        .read()
        .await
        .get(&id)
        .ok_or(PageError::NotFound)?
        .write()
        .await
        .config = config.clone();
    Ok(HttpResponse::Ok().body(format!("Edited {id} with {}", config.title)))
}

// TODO: Cache playlists

#[routes]
#[get("/{radio}/listen/master.m3u8")]
async fn hls_master(
    path: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse, PageError> {
    let id = path.into_inner();

    Ok(HttpResponse::Ok().body(
        state
            .radio_states
            .read()
            .await
            .get(&id)
            .ok_or(PageError::NotFound)?
            .read()
            .await
            .playlist
            .format_master(&format!("/{id}/listen/"), &BANDWIDTHS),
    ))
}

#[routes]
#[get("/{radio}/listen/{bandwidth}/playlist.m3u8")]
async fn hls_media(
    path: web::Path<(String, usize)>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse, PageError> {
    let (id, band) = path.into_inner();
    let i = BANDWIDTHS
        .iter()
        .enumerate()
        .find_map(|(i, b)| if b == &band { Some(i) } else { None })
        .ok_or(PageError::NotFound)?;

    Ok(HttpResponse::Ok().body(
        state
            .radio_states
            .read()
            .await
            .get(&id)
            .ok_or(PageError::NotFound)?
            .read()
            .await
            .playlist
            .format_media(i)
            .unwrap() // PANICKING: I is always a bandwidth used
            .clone(),
    ))
}

#[routes]
#[get("/{radio}/listen/{bandwidth}/{segment}.mp3")]
async fn hls_segment(
    path: web::Path<(String, usize, usize)>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse, PageError> {
    let (id, band, seg) = path.into_inner();
    Ok(
        HttpResponse::Ok().body(todo!()/*state.radio_states.read().await.get(&id).ok_or(PageError::NotFound)?.read().await.playlist*/),
    )
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();
    let port = args.port.unwrap_or(8080);
    let pages = if let Some(path) = args.pages {
        // Get Pages from Files
        let mut start_path = path.clone();
        let mut radio_path = path.clone();
        let mut edit_path = path.clone();
        start_path.push("start.html");
        radio_path.push("radio.html");
        edit_path.push("edit.html");
        // Read all files
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
    // Create Channels for communication between blocking and async
    let (atx, arx) = unbounded_channel();
    let (stx, srx) = unbounded_channel();
    // Create AppState
    let data: Arc<AppState> = Arc::new(AppState {
        pages,
        to_blocking: stx,
        radio_states: RwLock::new(HashMap::new()),
    });
    // Add test radio
    data.radio_states.write().await.insert(
        "test".to_owned(),
        RwLock::new(RadioState {
            config: Config {
                title: "Test".to_owned(),
                description: "This is a test station, \n ignore".to_owned(),
            },
            playlist: hls::MasterPlaylist::new([hls::MediaPlaylist::new([hls::Segment {}])]),
        }),
    );

    // Start blocking thread
    std::thread::spawn(|| blocking::main(atx, srx, Duration::from_secs(10)));

    // Start web server task
    let server = {
        let data = data.clone();
        HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(data.clone()))
                .service(start_page)
                .service(auth_page)
                .service(radio_page)
                .service(radio_edit)
                .service(radio_config)
                .service(hls_master)
                .service(hls_media)
                .service(hls_segment)
                .service(Files::new("/reserved", "./resources").prefer_utf8(true))
        })
        .bind(("0.0.0.0", port))?
        .run()
    };

    // Start HLS worker task
    let hls = {
        let data = data.clone();
        tokio::task::spawn(
            UnboundedReceiverStream::new(arx)
                .then(move |frame| hls::update(frame.0, frame.1, data.clone()))
                .collect::<()>(),
        )
    };
    // Run all tasks (until one finishes)
    // NOTE: Only use Futures that only finish on unrecoverable errors (but we still want to exit gracefully)
    select! {
    x = server => return x,
    _ = hls => unreachable!()
    }
}

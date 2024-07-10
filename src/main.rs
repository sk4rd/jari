use actix_files::Files;
use actix_web::{
    http::header::Expires,
    middleware::Compress,
    routes,
    web::{self},
    App, HttpResponse, HttpServer, Result,
};
use clap::Parser;
use futures::{future::join_all, StreamExt};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::SystemTime};
use tokio::{
    fs::read_to_string,
    select,
    sync::{mpsc::unbounded_channel, RwLock},
    time::Duration,
};
use tokio_stream::wrappers::UnboundedReceiverStream;

mod blocking;
use blocking::ToBlocking;

mod errors;
use errors::PageError;

mod handlers;
use handlers::*;

mod hls;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    port: Option<u16>,
    pages: Option<PathBuf>,
}

const NUM_BANDWIDTHS: usize = 1;
const NUM_SEGMENTS: usize = 2;

const BANDWIDTHS: [usize; NUM_BANDWIDTHS] = [22000];
/// Radio Config sent by the frontend
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    title: String,
    description: String,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PartialConfig {
    title: Option<String>,
    description: Option<String>,
}
/// Data for the radios
#[derive(Debug, Clone)]
pub struct RadioState {
    config: Config,
    playlist: hls::MasterPlaylist<NUM_BANDWIDTHS, NUM_SEGMENTS>,
    song_map: HashMap<String, u8>,
    song_order: Vec<String>,
}
/// Global async app state
pub struct AppState {
    pages: (&'static str, &'static str, &'static str),
    to_blocking: tokio::sync::mpsc::UnboundedSender<ToBlocking>,
    radio_states: RwLock<HashMap<String, RwLock<RadioState>>>,
}

// TODO: Cache playlists

#[routes]
#[get("/{radio}/listen/master.m3u8")]
async fn hls_master(
    path: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse, PageError> {
    let id = path.into_inner();

    Ok(HttpResponse::Ok()
        .insert_header(("Content-Type", "audio/mpegurl"))
        .body(
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

    Ok(HttpResponse::Ok()
        .insert_header(("Content-Type", "audio/mpegurl"))
        .body(
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
    let i = BANDWIDTHS
        .iter()
        .enumerate()
        .find_map(|(i, b)| if b == &band { Some(i) } else { None })
        .ok_or(PageError::NotFound)?;
    let radio_states_read = state.radio_states.read().await;
    let radio_state = radio_states_read
        .get(&id)
        .ok_or(PageError::NotFound)?
        .read()
        .await;
    Ok(HttpResponse::Ok()
        .insert_header(Expires(
            SystemTime::now()
                .checked_add(Duration::from_secs(
                    10 * (radio_state
                        .playlist
                        .current()
                        .checked_sub(seg)
                        .ok_or(PageError::NotFound)?) as u64 as u64,
                ))
                .ok_or(PageError::InternalError)?
                .into(),
        ))
        .body(actix_web::web::Bytes::from(
            radio_state
                .playlist
                .get_segment_raw(i, seg)
                .ok_or(PageError::NotFound)?,
        )))
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
            playlist: hls::MasterPlaylist::new([hls::MediaPlaylist::new([
                hls::Segment::new(
                    // NOTICE: This is a test segment taken from the Public Domain recording of Traditional American blues performed by Al Bernard & The Goofus Five year 1930
                    Box::new(include_bytes!("segment.mp3").clone()),
                ),
                hls::Segment::new(Box::new(include_bytes!("segment2.mp3").clone())),
            ])]),
            song_map: HashMap::new(),
            song_order: Vec::new(),
        }),
    );

    let mut blocking_radio_map = HashMap::new();
    blocking_radio_map.extend(
        data.radio_states
            .read()
            .await
            .iter()
            .map(|(name, _state)| (name.clone(), vec![])), // TODO: get order from file
    );

    let song_data_dir = PathBuf::from("./data");

    // Start blocking thread
    std::thread::spawn(|| {
        blocking::main(
            atx,
            srx,
            Duration::from_secs(10),
            blocking_radio_map,
            song_data_dir,
        )
    });

    // Start web server task
    let server = {
        let data = data.clone();
        HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(data.clone()))
                .wrap(Compress::default())
                .service(get_start_page)
                .service(get_auth_page)
                .service(get_radio_page)
                .service(get_radio_edit_page)
                .service(set_radio_config)
                .service(add_radio)
                .service(upload_song)
                .service(get_song_order)
                .service(set_song_order)
                .service(remove_radio)
                .service(remove_song)
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

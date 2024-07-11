use actix_files::Files;
use actix_web::{
    middleware::Compress,
    web::{self},
    App, HttpServer, Result,
};
use clap::Parser;
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

mod errors;

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
    pages: [String; 4],
    to_blocking: tokio::sync::mpsc::UnboundedSender<ToBlocking>,
    radio_states: RwLock<HashMap<String, RwLock<RadioState>>>,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();
    let port = args.port.unwrap_or(8080);
    let pages = if let Some(path) = args.pages {
        // Read all files
        let files = join_all([
            read_to_string(path.join("start.html")),
            read_to_string(path.join("radio.html")),
            read_to_string(path.join("edit.html")),
            read_to_string(path.join("login.html")),
        ])
        .await
        .into_iter()
        .collect::<Result<Box<_>, _>>()?;
        [
            files[0].clone(),
            files[1].clone(),
            files[2].clone(),
            files[3].clone(),
        ]
    } else {
        [
            include_str!("../resources/start.html"),
            include_str!("../resources/radio.html"),
            include_str!("../resources/edit.html"),
            include_str!("../resources/login.html"),
        ]
        .map(|s| s.to_owned())
    }
    .map(|s| {
        s.replace("./", "/reserved/")
            .replace("start.html", "/")
            .replace("login.html", "/auth")
    });
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
                .service(hls::get_master)
                .service(hls::get_media)
                .service(hls::get_segment)
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

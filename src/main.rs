use actix_files::Files;
use actix_web::{
    middleware::Compress,
    web::{self},
    App, HttpServer, Result,
};
use clap::{Parser, Subcommand};
use futures::{future::join_all, io::BufReader, join, StreamExt};
use itertools::Itertools;
use rustls::{internal::msgs::codec::Codec, Certificate, PrivateKey, ServerConfig};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::{
    fs::{read_to_string, File},
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
#[command(propagate_version = true)]
struct Args {
    #[arg(short, long)]
    port: Option<u16>,
    #[arg(short = 'P', long)]
    pages: Option<PathBuf>,
    #[arg(short, long)]
    threads: Option<usize>,
    #[command(subcommand)]
    tls: Option<TlsArgs>,
}

#[derive(Subcommand, Debug, Clone)]
enum TlsArgs {
    Files { cert: PathBuf, key: PathBuf },
}

const NUM_BANDWIDTHS: usize = 1;
const NUM_SEGMENTS: usize = 4;

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

fn main() -> std::io::Result<()> {
    let args = Args::parse();
    let port = args.port.unwrap_or(8080);
    let threads = args.threads.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map(|v| v.into())
            .unwrap_or(6)
    });
    let tls_opts = args.tls;
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(threads)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
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
                .collect::<Result<Box<[String]>, _>>()?;
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
                    playlist: hls::MasterPlaylist::default(),
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
                let server = HttpServer::new(move || {
                    App::new()
                        .app_data(web::Data::new(data.clone()))
                        .wrap(Compress::default())
                        .service(get_search_page)
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
                });
                match tls_opts {
                    Some(TlsArgs::Files { cert, key }) => {
                        use std::fs::File;
                        use std::io::BufReader;
                        let (cert, key) = (File::open(cert), File::open(key));
                        let (mut cert, mut key) = (BufReader::new(cert?), BufReader::new(key?));
                        let cert_chain = rustls_pemfile::certs(&mut cert)?
                            .into_iter()
                            .map(|buf| {
                                Certificate::read_bytes(&buf)
                                    .ok_or(std::io::Error::other("Invalid Cert"))
                            })
                            .try_collect()?;

                        let key = rustls_pemfile::pkcs8_private_keys(&mut key)?
                            .into_iter()
                            .map(PrivateKey)
                            .next()
                            .ok_or(std::io::Error::other("No keys"))?;
                        server.bind_rustls(
                            "0.0.0.0",
                            ServerConfig::builder()
                                .with_safe_defaults()
                                .with_no_client_auth()
                                .with_single_cert(cert_chain, key)
                                .expect("Invalid Tls Cert/Key"),
                        )?
                    }
                    None => {
                        if let (Some((_, cert)), Some((_, key))) = (
                            std::env::vars().find(|(name, _)| name == "SSL_CERT_FILE"),
                            std::env::vars().find(|(name, _)| name == "SSL_KEY_FILE"),
                        ) {
                            use std::fs::File;
                            use std::io::BufReader;
                            let (cert, key) = (File::open(cert), File::open(key));
                            let (mut cert, mut key) = (BufReader::new(cert?), BufReader::new(key?));
                            let cert_chain = rustls_pemfile::certs(&mut cert)?
                                .into_iter()
                                .map(|buf| {
                                    Certificate::read_bytes(&buf)
                                        .ok_or(std::io::Error::other("Invalid Cert"))
                                })
                                .try_collect()?;

                            let key = rustls_pemfile::pkcs8_private_keys(&mut key)?
                                .into_iter()
                                .map(PrivateKey)
                                .next()
                                .ok_or(std::io::Error::other("No keys"))?;
                            server.bind_rustls(
                                "0.0.0.0",
                                ServerConfig::builder()
                                    .with_safe_defaults()
                                    .with_no_client_auth()
                                    .with_single_cert(cert_chain, key)
                                    .expect("Invalid Tls Cert/Key"),
                            )?
                        } else {
                            server.bind(("0.0.0.0", port))?
                        }
                    }
                }
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
        })
}

use actix_files::Files;
use actix_web::{
    middleware::Compress,
    web::{self},
    App, HttpServer, Result,
};
use ammonia::clean;
use auth::{google_callback, google_redirect, OidcClient};
use clap::{Parser, Subcommand};
use futures::{future::join_all, StreamExt, TryFutureExt};
use itertools::Itertools;
use openidconnect::SubjectIdentifier;
use rustls::{pki_types::PrivateKeyDer, ServerConfig};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, ops::Deref, path::PathBuf, sync::Arc};
use tokio::{
    fs::read_to_string,
    select,
    sync::{mpsc, mpsc::unbounded_channel, oneshot, watch, RwLock},
    time::Duration,
};
use zbus::interface;

mod blocking;
use blocking::ToBlocking;

mod errors;

mod handlers;
use handlers::*;

mod auth;

mod cli;

#[derive(Parser, Debug)]
#[command(version, about)]
#[command(propagate_version = true)]
struct Args {
    #[arg(short, long)]
    port: Option<u16>,
    #[arg(short, long)]
    threads: Option<usize>,
    #[command(subcommand)]
    tls: Option<TlsArgs>,
    #[arg(short, long)]
    working_dir: Option<PathBuf>,
}

#[derive(Subcommand, Debug, Clone)]
enum TlsArgs {
    Files { cert: PathBuf, key: PathBuf },
}

#[derive(Clone, Debug)]
pub(crate) struct CleanString(String);

impl From<String> for CleanString {
    fn from(value: String) -> Self {
        Self(clean(&value))
    }
}
impl From<&String> for CleanString {
    fn from(value: &String) -> Self {
        Self(clean(value))
    }
}
impl From<&str> for CleanString {
    fn from(value: &str) -> Self {
        Self(clean(value))
    }
}
impl Into<String> for CleanString {
    fn into(self) -> String {
        self.0
    }
}
impl AsRef<str> for CleanString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
impl Deref for CleanString {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

const NUM_BANDWIDTHS: usize = 4;

const BANDWIDTHS: [usize; NUM_BANDWIDTHS] = [128000, 96000, 48000, 24000];
/// Radio Config
#[derive(Debug, Clone)]
pub struct Config {
    title: CleanString,
    description: CleanString,
}
/// Radio Config from frontend (not cleaned)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SentConfig {
    title: String,
    description: String,
}
/// Partial Radio config from Frontend (not cleaned)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PartialConfig {
    title: Option<String>,
    description: Option<String>,
}
/// Data for the radios
#[derive(Debug, Clone)]
pub struct RadioState {
    config: Config,
    stream: watch::Receiver<(Vec<u8>, [Vec<u8>; NUM_BANDWIDTHS])>,
    song_map: HashMap<String, u8>,
    song_order: Vec<String>,
    owner: SubjectIdentifier,
}
/// Serializable Data for saving radio state
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PersistentRadioState {
    config: SentConfig,
    song_map: HashMap<String, u8>,
    song_order: Vec<String>,
    owner: SubjectIdentifier,
}
/// Global async app state
#[derive(Debug)]
pub struct AppState {
    pages: RwLock<[String; 5]>,
    to_blocking: tokio::sync::mpsc::UnboundedSender<ToBlocking>,
    radio_states: RwLock<HashMap<String, RwLock<RadioState>>>,
    oidc_client: Arc<OidcClient>,
    users: RwLock<HashMap<SubjectIdentifier, Vec<String>>>,
}
/// Serializeble app state
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PersistentAppState {
    radio_states: HashMap<String, PersistentRadioState>,
    users: HashMap<SubjectIdentifier, Vec<String>>,
}

struct CliListener {
    state: Arc<AppState>,
    shutdown: Option<oneshot::Sender<()>>,
    save: mpsc::UnboundedSender<oneshot::Sender<()>>,
    reload_pages: mpsc::UnboundedSender<(PathBuf, oneshot::Sender<Result<(), String>>)>,
}

#[interface(name = "com.github.sk4rd.jari")]
impl CliListener {
    async fn remove_song(&self, radio: String, song: String) -> String {
        let radios_lock = self.state.radio_states.read().await;
        let Some(radio_lock) = radios_lock.get(&radio) else {
            return format!("can't remove song from radio {radio} because the radio doesn't exist");
        };
        let mut radio_lock = radio_lock.write().await;
        let Some(&song_id) = radio_lock.song_map.get(&song) else {
            return format!(
                "can't remove song {song} from radio {radio} because the song doesn't exist"
            );
        };

        radio_lock.song_map.remove(&song);
        radio_lock.song_order.retain(|name| name != &song);

        let Ok(()) = self.state.to_blocking.send(ToBlocking::Remove {
            radio: radio.clone(),
            song: song_id,
        }) else {
            eprintln!("Couldn't send message to blocking");
            return "Internal Error".to_owned();
        };
        format!("Removed song {song} from radio {radio}")
    }
    async fn remove_radio(&self, radio: String) -> String {
        let mut radios_lock = self.state.radio_states.write().await;
        let Some(_) = radios_lock.remove(&radio) else {
            return format!("Can't remove radio {radio} because it doesn't exist");
        };
        let Ok(()) = self.state.to_blocking.send(ToBlocking::RemoveRadio {
            radio: radio.clone(),
        }) else {
            eprintln!("Couldn't send message to blocking");
            return "Internal Error".to_owned();
        };
        format!("Removed radio {radio}")
    }
    async fn remove_user(&mut self, sub: String) -> String {
        let Some(_) = self
            .state
            .users
            .write()
            .await
            .remove(&SubjectIdentifier::new(sub.clone()))
        else {
            return format!("Err! No user with sub: {sub}");
        };
        format!("Removed user with sub: {sub}")
    }
    async fn add_user(&self, sub: String) -> String {
        let Ok(token) = jsonwebtoken::encode(
            &self.state.oidc_client.header,
            &auth::Claims {
                sub: SubjectIdentifier::new(sub.clone()),
                exp: jsonwebtoken::get_current_timestamp() as usize + 7 * 24 * 60 * 60,
            },
            &self.state.oidc_client.encoding_key,
        ) else {
            return format!("Couldn't generate token");
        };

        self.state
            .users
            .write()
            .await
            .entry(SubjectIdentifier::new(sub.clone()))
            .or_insert(vec![]);
        format!("Added user with sub {sub}. Token: {token}")
    }
    async fn list_users(&self) -> Vec<String> {
        self.state
            .users
            .read()
            .await
            .iter()
            .map(|(key, val)| format!("Sub: {}, radios: {val:?}", key.to_string()))
            .collect_vec()
    }
    async fn count_users(&self) -> usize {
        self.state.users.read().await.len()
    }
    async fn list_radios(&self) -> Vec<String> {
        self.state
            .radio_states
            .read()
            .await
            .keys()
            .cloned()
            .collect()
    }
    async fn list_songs(&self, radio: String) -> Vec<String> {
        let radios_lock = self.state.radio_states.read().await;
        let Some(radio_lock) = radios_lock.get(&radio) else {
            return vec![];
        };
        let res = radio_lock.read().await.song_map.keys().cloned().collect();
        res
    }
    async fn reload_pages(&self, path: PathBuf) -> String {
        let (tx, rx) = oneshot::channel();
        let Ok(()) = self.reload_pages.send((path.clone(), tx)) else {
            return format!("Couldn't send reload message");
        };
        let Ok(res) = rx.await else {
            return format!("Couldn't recieve reload confirmation");
        };
        if let Err(e) = res {
            return e;
        };
        format!("Reloaded pages from {path:?}")
    }
    async fn print_state(&self) -> String {
        format!("{:?}", self.state)
    }
    async fn save(&self) -> String {
        let (tx, rx) = oneshot::channel();
        let Ok(()) = self.save.send(tx) else {
            return format!("Couldn't send save message");
        };
        let Ok(()) = rx.await else {
            return format!("Couldn't recieve save confirmation");
        };
        format!("Saved state")
    }
    async fn shutdown(&mut self) -> String {
        let Some(tx) = self.shutdown.take() else {
            return format!("Couldn't shut down");
        };
        let _ = tx.send(());
        format!("Shutting down!")
    }
}

async fn cli_listener(state: Arc<AppState>, data_dir: PathBuf) -> zbus::Result<()> {
    use zbus::connection;

    let (tx, rx) = oneshot::channel();
    let (tx_save, rx_save) = mpsc::unbounded_channel();
    let (tx_reload, rx_reload) = mpsc::unbounded_channel();
    let save_stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx_save)
        .then(|tx: oneshot::Sender<()>| async {
            save_state(state.clone(), data_dir.clone()).await;
            if let Err(()) = tx.send(()) {
                eprintln!("Couldn't send save confirmation")
            };
        })
        .collect::<()>();
    let reload_stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx_reload)
        .then(
            |(path, tx): (PathBuf, oneshot::Sender<Result<(), String>>)| async {
                let res = match load_pages(Some(path)).await {
                    Err(e) => Err(format!("Couldn't load pages {e}")),
                    Ok(pages) => Ok(*state.pages.write().await = pages),
                };

                if let Err(_) = tx.send(res) {
                    eprintln!("Couldn't send save confirmation")
                };
            },
        )
        .collect::<()>();
    let listener = CliListener {
        state: state.clone(),
        shutdown: Some(tx),
        save: tx_save,
        reload_pages: tx_reload,
    };
    let _connection = connection::Builder::session()?
        .name("com.github.sk4rd.jari")?
        .serve_at("/com/github/sk4rd/jari", listener)?
        .build()
        .await?;
    select! {
        x = rx => x.map_err(|_| zbus::Error::Failure("channel broke".to_owned()))?,
        x = save_stream => x,
        x = reload_stream => x
    }
    Ok(())
}

async fn save_state(data: Arc<AppState>, data_dir: PathBuf) {
    let mut radio_states = data.radio_states.write().await;
    let mut persistent_radio_states = HashMap::new();
    for (name, radio_state) in radio_states.iter_mut() {
        let RadioState {
            config,
            stream: _,
            song_map,
            song_order,
            owner,
        } = radio_state.get_mut().clone();
        persistent_radio_states.insert(
            name.clone(),
            PersistentRadioState {
                config: SentConfig {
                    title: config.title.into(),
                    description: config.description.into(),
                },
                song_map,
                song_order,
                owner,
            },
        );
    }
    let state = PersistentAppState {
        radio_states: persistent_radio_states,
        users: data.users.read().await.clone(),
    };
    let state_buf = postcard::to_allocvec(&state).unwrap();
    tokio::fs::write(data_dir.join("state"), state_buf)
        .await
        .unwrap();
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
            let pages = load_pages(args.working_dir.clone()).await?;
            // Create Channels for communication between blocking and async
            let (stx, srx) = unbounded_channel();

            let oidc_client = Arc::new(OidcClient::new().await);

            // Create AppState
            let data: Arc<AppState> = Arc::new(AppState {
                pages: RwLock::new(pages),
                to_blocking: stx,
                radio_states: RwLock::new(HashMap::new()),
                oidc_client,
                users: RwLock::new(HashMap::new()),
            });

            let data_dir = args.working_dir.clone().map(|d| d.join("data")).unwrap_or(PathBuf::from("./data"));
            // Load radio state
            let mut blocking_radio_map = HashMap::new();
            if let Ok(state_file) = tokio::fs::read(data_dir.join("state")).await {
                let loaded_state: PersistentAppState =
                    postcard::from_bytes(&state_file).expect("State file has invalid data!");
                for (
                    name,
                    PersistentRadioState {
                        config,
                        song_map,
                        song_order,
                        owner
                    },
                ) in loaded_state.radio_states.into_iter()
                {
                    let (tx, rx) = watch::channel((vec![], [(); NUM_BANDWIDTHS].map(|_| vec![])));
                    blocking_radio_map.insert(
                        name.clone(),
                        (
                            song_order
                                .iter()
                                .filter_map(|song| song_map.get(song).copied())
                                .collect(),
                            tx,
                        ),
                    );
                    data.radio_states.write().await.insert(
                        name,
                        RwLock::new(RadioState {
                            config: Config {
                                title: config.title.into(),
                                description: config.description.into(),
                            },
                            stream: rx,
                            song_map,
                            song_order,
                            owner
                        }),
                    );
                }
                *data.users.write().await = loaded_state.users;
            }

            // Start blocking thread
            let blocking_data_dir = data_dir.clone();
            std::thread::spawn(|| {
                blocking::main(
                    srx,
                    Duration::from_secs(10),
                    blocking_radio_map,
                    blocking_data_dir,
                )
            });

            // Start web server task
            let server = {
                let data = data.clone();
                let working_dir = Arc::new(args.working_dir.clone());
                let server = HttpServer::new(move || {
                    App::new()
                        .app_data(web::Data::new(data.clone()))
                        .wrap(Compress::default())
                        .service(get_search_page)
                        .service(get_start_page)
                        .service(get_auth_page)
                        .service(get_radio_page)
                        .service(get_radio_edit_page)
                        .service(get_settings_page)
                        .service(set_radio_config)
                        .service(add_radio)
                        .service(upload_song)
                        .service(get_songs)
                        .service(get_song_order)
                        .service(set_song_order)
                        .service(remove_user)
                        .service(remove_radio)
                        .service(remove_song)
                        .service(get_audio)
                        .service(get_audio_band)
                        .service(google_redirect)
                        .service(google_callback)
                        .service(Files::new("/reserved", (*working_dir).clone().unwrap_or(PathBuf::from(".")).join("resources")).prefer_utf8(true))
                });
                let tls_env = (
                    std::env::vars()
                        .find(|(name, _)| name == "SSL_CERT_FILE")
                        .map(|v| PathBuf::from(v.1)),
                    std::env::vars()
                        .find(|(name, _)| name == "SSL_KEY_FILE")
                        .map(|v| PathBuf::from(v.1)),
                );
                match (tls_opts, tls_env) {
                    (Some(TlsArgs::Files { cert, key }), _) | (_, (Some(cert), Some(key))) => {
                        use std::fs::File;
                        use std::io::BufReader;
                        let (cert, key) = (File::open(cert), File::open(key));
                        let (mut cert, mut key) = (BufReader::new(cert?), BufReader::new(key?));
                        let cert_chain = rustls_pemfile::certs(&mut cert).try_collect()?;

                        let key = rustls_pemfile::pkcs8_private_keys(&mut key)
                            .map(|key| key.map(PrivateKeyDer::Pkcs8))
                            .next()
                            .ok_or(std::io::Error::other("No keys"))??;
                        server.bind_rustls_0_23(
                            ("0.0.0.0", port),
                            ServerConfig::builder()
                                .with_no_client_auth()
                                .with_single_cert(cert_chain, key)
                                .expect("Invalid Tls Cert/Key"),
                        )?
                    }
                    _ => server.bind(("0.0.0.0", port))?,
                }
                .run()
            };

            let cli = cli_listener(data.clone(), data_dir.clone());

            // Run all tasks (until one finishes)
            // NOTE: Only use Futures that only finish on unrecoverable errors (but we still want to exit gracefully)
            let res = select! {
            x = server => x,
            x = cli.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string())) => {eprintln!("Cli shutdown"); x},
            };
            // Save radio states
            save_state(data, data_dir).await;
            res
        })
}

async fn load_pages(pages: Option<PathBuf>) -> std::io::Result<[String; 5]> {
    Ok({
        let path = pages.unwrap_or(PathBuf::from(".")).join("resources");
        // Read all files
        let files = join_all([
            read_to_string(path.join("start.html")),
            read_to_string(path.join("radio.html")),
            read_to_string(path.join("edit.html")),
            read_to_string(path.join("login.html")),
            read_to_string(path.join("settings.html")),
        ])
        .await
        .into_iter()
        .collect::<Result<Box<[String]>, _>>()?;
        [
            files[0].clone(),
            files[1].clone(),
            files[2].clone(),
            files[3].clone(),
            files[4].clone(),
        ]
    }
    .map(|s| {
        s.replace("./", "/reserved/")
            .replace("start.html", "/")
            .replace("login.html", "/auth")
    }))
}

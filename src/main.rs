use actix_files::Files;
use actix_multipart::Multipart;
use actix_web::{
    body::MessageBody,
    delete,
    error::ResponseError,
    get,
    http::{header::Expires, StatusCode},
    middleware::Compress,
    put, routes,
    web::{self, Form},
    App, HttpResponse, HttpServer, Responder, Result,
};
use clap::Parser;
use derive_more::{Display, Error};
use futures::{future::join_all, StreamExt};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::SystemTime};
use tokio::{
    fs::read_to_string,
    select,
    sync::{mpsc::unbounded_channel, RwLock},
    time::Duration,
};
use tokio_stream::{wrappers::UnboundedReceiverStream, Timeout};

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
    #[display(fmt = "Internal server error")]
    InternalError,
    #[display(fmt = "Error handling multipart data")]
    MultipartError,
}

impl ResponseError for PageError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            PageError::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
            PageError::MultipartError => StatusCode::BAD_REQUEST,
        }
    }
}

impl From<tokio::sync::mpsc::error::SendError<ToBlocking>> for PageError {
    fn from(_: tokio::sync::mpsc::error::SendError<ToBlocking>) -> Self {
        PageError::InternalError
    }
}

impl From<actix_multipart::MultipartError> for PageError {
    fn from(_: actix_multipart::MultipartError) -> Self {
        PageError::MultipartError
    }
}

const NUM_BANDWIDTHS: usize = 1;
const NUM_SEGMENTS: usize = 2;

const BANDWIDTHS: [usize; NUM_BANDWIDTHS] = [22000];
/// Radio Config sent by the frontend
#[derive(Debug, Clone, Deserialize, Serialize)]
struct Config {
    title: String,
    description: String,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
struct PartialConfig {
    title: Option<String>,
    description: Option<String>,
}
/// Data for the radios
#[derive(Debug, Clone)]
struct RadioState {
    config: Config,
    playlist: hls::MasterPlaylist<NUM_BANDWIDTHS, NUM_SEGMENTS>,
    song_map: HashMap<String, u8>,
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
    web::Json(partial_config): web::Json<PartialConfig>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse, PageError> {
    let id = path.into_inner();
    let radio_states = state.radio_states.write().await;
    let radio_state = radio_states.get(&id).ok_or(PageError::NotFound)?;

    let mut radio_state_locked = radio_state.write().await;
    if let Some(title) = &partial_config.title {
        radio_state_locked.config.title = title.clone();
    }
    if let Some(description) = &partial_config.description {
        radio_state_locked.config.description = description.clone();
    }

    Ok(HttpResponse::Ok().body(format!(
        "Edited {id} with title: {}",
        radio_state_locked.config.title
    )))
}

#[put("/{radio}")]
async fn add_radio(
    path: web::Path<String>,
    web::Json(config): web::Json<Config>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse, PageError> {
    let id = path.into_inner();
    let mut radio_states = state.radio_states.write().await;

    if radio_states.contains_key(&id) {
        return Err(PageError::NotFound.into());
    }

    let new_radio_state = RadioState {
        config,
        playlist: hls::MasterPlaylist::default(),
        song_map: HashMap::new(),
    };

    radio_states.insert(id.clone(), RwLock::new(new_radio_state));
    state
        .to_blocking
        .send(ToBlocking::AddRadio { radio: id.clone() })
        .map_err(PageError::from)?;

    Ok(HttpResponse::Created().body(format!("Radio added with ID: {}", id)))
}

#[routes]
#[put("/{radio}/songs/{song}")]
#[put("/{radio}/songs/{song}/")]
async fn upload_song(
    path: web::Path<(String, String)>,
    mut payload: Multipart,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse, PageError> {
    let (radio_id, song_id) = path.into_inner();
    let mut song_data: Vec<u8> = Vec::new();

    // Process each part in the multipart payload
    while let Some(item) = payload.next().await {
        let mut field = item.map_err(|_| PageError::MultipartError)?;

        // Handle the content disposition to correctly find the file part
        if let Some(content_disposition) = field.content_disposition() {
            if content_disposition.get_name() == Some("file") {
                // Read the file data part-by-part
                while let Some(chunk) = field.next().await {
                    let data = chunk.map_err(|_| PageError::MultipartError)?;
                    song_data.extend_from_slice(&data);
                }
            }
        }
    }

    state
        .to_blocking
        .send(ToBlocking::Upload {
            radio: radio_id.clone(),
            song: song_id.clone().parse::<u8>().unwrap(),
            data: song_data.into_boxed_slice(),
        })
        .map_err(PageError::from)?;

    // Send a confirmation response
    Ok(HttpResponse::Ok().body(format!(
        "Song '{}' successfully uploaded to radio '{}'.",
        song_id, radio_id
    )))
}

#[routes]
#[put("/{radio}/order")]
#[put("/{radio}/order/")]
async fn set_order(
    path: web::Path<String>,
    payload: web::Json<Vec<String>>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse, PageError> {
    let radio_id = path.into_inner();
    let radio_states = state.radio_states.read().await;
    let radio_state = radio_states
        .get(&radio_id)
        .ok_or(PageError::NotFound)?
        .read()
        .await;

    state
        .to_blocking
        .send(ToBlocking::Order {
            radio: radio_id.clone(),
            order: payload
                .into_inner()
                .into_iter()
                .map(|name| radio_state.song_map.get(&name).cloned())
                .collect::<Option<Vec<u8>>>()
                .ok_or(PageError::NotFound)?,
        })
        .unwrap();

    Ok(HttpResponse::Ok().body(format!("Update song order of radio with ID {}", radio_id)))
}

#[delete("/{radio}")]
async fn remove_radio(
    path: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse, PageError> {
    let id = path.into_inner();
    let mut radio_states = state.radio_states.write().await;

    // Throw NotFound if page with id was not found
    if !radio_states.contains_key(&id) {
        return Err(PageError::NotFound.into());
    }

    radio_states.remove(&id);
    state
        .to_blocking
        .send(ToBlocking::RemoveRadio { radio: id.clone() })
        .map_err(PageError::from)?;

    Ok(HttpResponse::Ok().body(format!("Radio with ID {} has been removed", id)))
}

#[delete("/{radio}/songs/{song}")]
async fn remove_radio_song(
    path: web::Path<(String, String)>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse, PageError> {
    let (radio_id, song_name) = path.into_inner();
    let radio_states = state.radio_states.read().await;
    let radio_state = radio_states
        .get(&radio_id)
        .ok_or(PageError::NotFound)?
        .read()
        .await;

    state
        .to_blocking
        .send(ToBlocking::Remove {
            radio: radio_id.clone(),
            song: radio_state
                .song_map
                .get(&song_name)
                .ok_or(PageError::NotFound)?
                .clone(),
        })
        .expect("Couldn't send to backend");

    Ok(HttpResponse::Ok().body(format!(
        "Remove song '{}' from radio with ID {}",
        song_name, radio_id
    )))
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

    // Start blocking thread
    std::thread::spawn(|| blocking::main(atx, srx, Duration::from_secs(10), blocking_radio_map));

    // Start web server task
    let server = {
        let data = data.clone();
        HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(data.clone()))
                .wrap(Compress::default())
                .service(start_page)
                .service(auth_page)
                .service(radio_page)
                .service(radio_edit)
                .service(radio_config)
                .service(add_radio)
                .service(upload_song)
                .service(set_order)
                .service(remove_radio)
                .service(remove_radio_song)
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

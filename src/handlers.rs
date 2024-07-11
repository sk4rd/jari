use crate::blocking::ToBlocking;
use crate::errors::PageError;
use crate::hls::MasterPlaylist;
use crate::{AppState, Config, PartialConfig, RadioState};
use actix_multipart::Multipart;
use actix_web::{
    delete, put, routes,
    web::{self},
    HttpResponse, Responder,
};
use futures::StreamExt;
use itertools::Itertools;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

#[routes]
#[get("/")]
#[get("/index.html")]
pub async fn get_start_page(state: web::Data<Arc<AppState>>) -> impl Responder {
    let mut page = state.pages[0].clone();
    if let Some(start) = page.find("{radios}") {
        if let Some(end) = page.find("{radios-end}") {
            let snippet = (&page[start..end])
                .to_owned()
                .replace("{radios}", "")
                .replace("{radios-end}", "");
            let radio_states = state.radio_states.read().await;
            let radios = radio_states
                .iter()
                .map(|(id, data)| async {
                    let RadioState {
                        config: Config { title, description },
                        ..
                    } = &*data.read().await;
                    snippet
                        .replace("{id}", id)
                        .replace("{title}", title)
                        .replace("{description}", description)
                })
                .collect::<Vec<_>>();
            let mut radio_text = String::new();
            for radio in radios {
                let radio = radio.await;
                radio_text.push_str(&radio);
            }
            page.replace_range(start..end, &radio_text);
        }
    }
    HttpResponse::Ok().body(page.replace("{radios-end}", ""))
}

#[routes]
#[get("/auth")]
#[get("/auth/")]
pub async fn get_auth_page(state: web::Data<Arc<AppState>>) -> impl Responder {
    HttpResponse::Ok().body(state.pages[3].clone())
}

#[routes]
#[get("/{radio}")]
#[get("/{radio}/")]
#[get("/{radio}/index.html")]
pub async fn get_radio_page(
    path: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse, PageError> {
    let id = path.into_inner();
    // Extract Radio State
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
    // Return formatted data
    Ok(HttpResponse::Ok().body(
        state.pages[1]
            .replace("{title}", &title)
            .replace("{id}", &id)
            .replace("{description}", &description),
    ))
}

#[routes]
#[get("/{radio}/edit")]
#[get("/{radio}/edit/")]
#[get("/{radio}/edit/index.html")]
pub async fn get_radio_edit_page(
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
        state.pages[2]
            .replace("{title}", &title)
            .replace("{id}", &id)
            .replace("{description}", &description),
    ))
}

#[routes]
#[post("/{radio}")]
#[post("/{radio}/")]
pub async fn set_radio_config(
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
pub async fn add_radio(
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
        playlist: MasterPlaylist::default(),
        song_map: HashMap::new(),
        song_order: Vec::new(),
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
pub async fn upload_song(
    path: web::Path<(String, String)>,
    mut payload: Multipart,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse, PageError> {
    let (radio_id, song_id) = path.into_inner();
    let mut song_data: Vec<u8> = Vec::new();
    let radio_states = state.radio_states.read().await;

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

    let mut radio_state = radio_states
        .get(&radio_id)
        .ok_or(PageError::NotFound)?
        .write()
        .await;

    let id = radio_state
        .song_map
        .values()
        .sorted()
        .fold(0, |a, e| if *e == a { e + 1 } else { a });

    radio_state.song_map.insert(song_id.clone(), id);

    state
        .to_blocking
        .send(ToBlocking::Upload {
            radio: radio_id.clone(),
            song: id,
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
#[get("/{radio}/order")]
#[get("/{radio}/order/")]
pub async fn get_song_order(
    path: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> Result<web::Json<Vec<String>>, PageError> {
    let radio_id = path.into_inner();
    let radio_states = state.radio_states.read().await;
    let radio_state = radio_states
        .get(&radio_id)
        .ok_or(PageError::NotFound)?
        .read()
        .await;

    Ok(web::Json(radio_state.song_order.clone()))
}

#[routes]
#[put("/{radio}/order")]
#[put("/{radio}/order/")]
pub async fn set_song_order(
    path: web::Path<String>,
    payload: web::Json<Vec<String>>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse, PageError> {
    let radio_id = path.into_inner();
    let radio_states = state.radio_states.read().await;
    let mut radio_state = radio_states
        .get(&radio_id)
        .ok_or(PageError::ResourceNotFound)?
        .write()
        .await;

    radio_state.song_order = payload.into_inner();

    state
        .to_blocking
        .send(ToBlocking::Order {
            radio: radio_id.clone(),
            order: radio_state
                .song_order
                .iter()
                .map(|name| radio_state.song_map.get(name).cloned())
                .collect::<Option<Vec<u8>>>()
                .ok_or(PageError::NotFound)?,
        })
        .unwrap();

    Ok(HttpResponse::Ok().body(format!("Update song order of radio with ID {}", radio_id)))
}

#[delete("/{radio}")]
pub async fn remove_radio(
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
pub async fn remove_song(
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

use std::{
    collections::HashMap,
    fs::{create_dir, remove_dir_all},
    path::PathBuf,
    time::Duration,
};

use tokio::time::Instant;

use crate::hls::Segment;

/// Messages, that can be sent to the blocking thread (mainly audio)
#[derive(Debug, Clone)]
pub enum ToBlocking {
    /// Upload a song to segment and save (given a song id)
    Upload {
        radio: String,
        song: u8,
        data: Box<[u8]>,
    },
    /// Set a playlist order (order of song ids)
    Order { radio: String, order: Vec<u8> },
    /// Remove a song
    Remove { radio: String, song: u8 },
    /// Remove a radio
    RemoveRadio { radio: String },
    /// Add a radio
    AddRadio { radio: String },
}
/// The blocking thread, contains mainly audio processing
pub fn main(
    atx: tokio::sync::mpsc::UnboundedSender<(
        Instant,
        Vec<(String, [Segment; crate::NUM_BANDWIDTHS])>,
    )>,
    mut srx: tokio::sync::mpsc::UnboundedReceiver<ToBlocking>,
    interval: Duration,
    mut radios: HashMap<String, Vec<u8>>,
    root_dir: PathBuf,
) {
    let mut last = std::time::Instant::now();
    let _start = last.clone();
    let seg = Segment::new(Box::new(include_bytes!("segment2.mp3").clone()));
    loop {
        // Check for messages
        'mesg_check: {
            match srx.try_recv() {
                Ok(msg) => match msg {
                    ToBlocking::Upload { radio, song, data } => {
                        let Ok(()) = create_dir(root_dir.join(&radio).join(song.to_string()))
                        else {
                            eprintln!("Couldn't create dir for song {song} in radio {radio} with root {}!", root_dir.display());
                            break 'mesg_check;
                        };
                        // TODO(audio): save songs (batching)
                    }
                    ToBlocking::Order { radio, order } => {
                        let Some(radio_lock) = radios.get_mut(&radio) else {
                            eprintln!("Tried to set the order for non-existent radio {radio}!");
                            break 'mesg_check;
                        };
                        *radio_lock = order;
                    }
                    ToBlocking::Remove { radio, song } => {
                        let Some(radio_lock) = radios.get_mut(&radio) else {
                            eprintln!("Tried to remove song from non-existent radio {radio}!");
                            break 'mesg_check;
                        };
                        radio_lock.retain(|e| e != &song);
                        let Ok(()) = remove_dir_all(root_dir.join(&radio).join(song.to_string()))
                        else {
                            eprintln!("Couldn't remove dir for song {song} in radio {radio} with root {}!", root_dir.display());
                            break 'mesg_check;
                        };
                    }
                    ToBlocking::RemoveRadio { radio } => {
                        radios.remove(&radio);
                        let Ok(()) = remove_dir_all(root_dir.join(&radio)) else {
                            eprintln!(
                                "Couldn't remove dir for radio {radio} with root {}!",
                                root_dir.display()
                            );
                            break 'mesg_check;
                        };
                    }
                    ToBlocking::AddRadio { radio } => {
                        radios.insert(radio.clone(), vec![]);
                        let Ok(()) = create_dir(root_dir.join(&radio)) else {
                            eprintln!(
                                "Couldn't create dir for radio {radio} with root {}!",
                                root_dir.display()
                            );
                            break 'mesg_check;
                        };
                    }
                },
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => return,
            }
        }
        // Check if interval has been reached
        let diff = last.elapsed();
        if diff > interval {
            // TODO: send/create next fragment
            last += interval;
            atx.send((
                last.clone().into(),
                vec![("test".to_string(), [seg.clone()])],
            ))
            .unwrap();
        }
    }
}

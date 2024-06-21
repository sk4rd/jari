use std::time::Duration;

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
pub fn main<const S: usize>(
    _atx: tokio::sync::mpsc::UnboundedSender<(Instant, Vec<(String, [Segment; S])>)>,
    mut srx: tokio::sync::mpsc::UnboundedReceiver<ToBlocking>,
    interval: Duration,
) {
    let mut last = std::time::Instant::now();
    loop {
        // Check for messages
        match srx.try_recv() {
            Ok(msg) => match msg {
                // TODO(audio): handle messages
                _ => {
                    println!("{msg:?}");
                }
            },
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => return,
        }
        // Check if interval has been reached
        let diff = last.elapsed();
        if diff > interval {
            // TODO: send/create next fragment
            last += interval;
            _atx.send((last.clone().into(), vec![])).unwrap();
        }
    }
}

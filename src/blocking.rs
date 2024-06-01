use std::time::Duration;

use tokio::time::Instant;

use crate::hls::Segment;

/// Messages, that can be sent to the blocking thread (mainly audio)
pub enum ToBlocking {}
/// The blocking thread, contains mainly audio processing
pub fn main<const S: usize>(
    _atx: tokio::sync::mpsc::UnboundedSender<(Instant, Vec<[Segment; S]>)>,
    mut srx: tokio::sync::mpsc::UnboundedReceiver<ToBlocking>,
    interval: Duration,
) {
    let mut last = std::time::Instant::now();
    loop {
        // Check for messages
        match srx.try_recv() {
            Ok(msg) => match msg {},
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
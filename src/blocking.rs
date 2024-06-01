use std::time::Duration;

use tokio::time::Instant;

/// Messages, that can be sent to the blocking thread (mainly audio)
pub enum ToBlocking {}
/// The blocking thread, contains mainly audio processing
pub fn main(
    _atx: tokio::sync::mpsc::UnboundedSender<Instant>,
    srx: std::sync::mpsc::Receiver<ToBlocking>,
    interval: Duration,
) {
    let mut last = std::time::Instant::now();
    loop {
        // Check for messages
        match srx.try_recv() {
            Ok(msg) => match msg {},
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
        }
        // Check if interval has been reached
        let diff = last.elapsed();
        if diff > interval {
            // TODO: send/create next fragment
            last += interval;
            _atx.send(last.clone().into()).unwrap();
        }
    }
}

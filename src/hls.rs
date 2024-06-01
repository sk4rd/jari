use std::sync::Arc;

use tokio::time::Instant;

use crate::AppState;

pub struct MasterPlaylist<const P: usize, const S: usize> {
    playlists: [MediaPlaylist<S>; P],
}

impl<const P: usize, const S: usize> MasterPlaylist<P, S> {
    pub const fn new(playlists: [MediaPlaylist<S>; P]) -> Self {
        Self { playlists }
    }
    pub fn add_segments(&mut self, segments: [Segment; P]) {
        self.playlists
            .iter_mut()
            .zip(segments)
            .for_each(|(playlist, segment)| playlist.add_segment(segment));
    }
    pub fn format_master(&self) -> String {
        todo!()
    }
    pub fn format_media(&self) -> [String; P] {
        let mut out = [""; P].map(|s| s.to_owned());
        for i in 0..P {
            out[i] = self.playlists[i].format();
        }
        out
    }
}

struct MediaPlaylist<const S: usize> {
    current: usize,
    segments: [Segment; S],
}

impl<const S: usize> MediaPlaylist<S> {
    const fn new(segments: [Segment; S]) -> Self {
        Self {
            current: S - 1,
            segments,
        }
    }
    fn add_segment(&mut self, segment: Segment) {
        let i = if self.current < S - 1 {
            self.current + 1
        } else {
            0
        };
        self.segments[i] = segment;
    }
    fn format(&self) -> String {
        todo!()
    }
}

pub struct Segment {}

/// Function to add the new segments and set the new current segment
pub async fn update<const S: usize>(_instant: (Instant, Vec<[Segment; S]>), _data: Arc<AppState>) {
    // TODO: Update the HLS data on to instant
    println!("{}µs", _instant.0.elapsed().as_micros())
}

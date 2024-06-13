use std::sync::Arc;

use tokio::time::Instant;

use crate::AppState;

/// The master playlist, contains its media playlists (P is amount of playlists/bandwidths, S is amount of Segments per media playlist) (S > 0)
#[derive(Debug, Clone)]
pub struct MasterPlaylist<const P: usize, const S: usize> {
    playlists: [MediaPlaylist<S>; P],
}

impl<const P: usize, const S: usize> MasterPlaylist<P, S> {
    /// Create a new MasterPlaylist from its MediaPlaylists
    pub const fn new(playlists: [MediaPlaylist<S>; P]) -> Self {
        Self { playlists }
    }
    /// Add a segment to each MediaPlaylist
    pub fn add_segments(&mut self, segments: [Segment; P]) {
        self.playlists
            .iter_mut()
            .zip(segments)
            .for_each(|(playlist, segment)| playlist.add_segment(segment));
    }
    /// Produce a formatted string in m3u8 format of the master playlist
    pub fn format_master(&self, base_path: &str, bandwidths: &[usize; P]) -> String {
        // TODO: Confirm/Test this
        // Format the metadata for each playlist/bandwidth
        let playlist_descrs = bandwidths.iter().map(|bandwidth| {
            format!(
                "#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"{bandwidth}\",NAME=\"{bandwidth}\",AUTOSELECT=YES,DEFAULT=YES
#EXT-X-STREAM-INF:BANDWIDTH={bandwidth},CODECS=\"mp3\"
{base_path}{bandwidth}/playlist.m3u8"
            )
        });
        // Combine all metadata with header
        format!(
            "#EXTM3U
{}",
            playlist_descrs
                .reduce(|a, e| format!("{a}\n{e}"))
                .unwrap_or(String::new())
        )
    }
    /// Format each of the media playlists
    pub fn format_media(&self) -> [String; P] {
        let mut out = [""; P].map(|s| s.to_owned());
        for i in 0..P {
            out[i] = self.playlists[i].format();
        }
        out
    }
}

/// The media playlist, normally of a specific bandwidth, contains its segments with indeces
/// (S is the amount of segments it can store, this cannot be 0)
#[derive(Debug, Clone)]
pub struct MediaPlaylist<const S: usize> {
    current_index: usize,
    current: usize,
    segments: [Segment; S],
}

impl<const S: usize> MediaPlaylist<S> {
    /// Check the compiletime requirements (S > 0)
    const _TESTS: () = {
        assert!(S > 0);
    };
    /// Create a MediaPlaylist from its Segments
    pub const fn new(segments: [Segment; S]) -> Self {
        Self {
            current_index: S - 1,
            current: 0,
            segments,
        }
    }
    /// Add a Segment dropping the oldest
    pub fn add_segment(&mut self, segment: Segment) {
        let i = if self.current_index < S - 1 {
            self.current_index + 1
        } else {
            0
        };
        self.segments[i] = segment;
    }
    /// Produce a formatted m3u8 String for the media playlist
    pub fn format(&self) -> String {
        // TODO: Confirm/Test this
        let start = if self.current >= S {
            self.current - S
        } else {
            0
        };
        let segment_descrs = (0..S).map(|i| {
            format!(
                "#EXTINF:10.000,
{i}.acc"
            )
        });
        format!(
            "#EXTM3U
#EXT-X-VERSION:3
#EXT-X-TARGETDURATION:10
#EXT-X-PLAYLIST-TYPE:EVENT
#EXT-X-MEDIA-SEQUENCE:{start}
{}",
            segment_descrs
                .reduce(|a, e| format!("{a}\n{e}"))
                .unwrap_or(String::new())
        )
    }
}

/// A HLS Segment, should contain audio data with header
#[derive(Debug, Clone)]
pub struct Segment {}

/// Function to add the new segments and set the new current segment
pub async fn update(
    instant: Instant,
    audio: Vec<(String, [Segment; crate::NUM_BANDWIDTHS])>,
    data: Arc<AppState>,
) {
    // TODO: Update the HLS data on to instant
    for (id, segments) in audio {
        let radio_states = data.radio_states.read().await;
        let Some(state) = radio_states.get(&id) else {
            eprintln!("Mismatched State! {id} was sent by blocking, but is not in appstate");
            continue;
        };
        state.write().await.playlist.add_segments(segments);
    }
    println!("{}µs", instant.elapsed().as_micros())
}

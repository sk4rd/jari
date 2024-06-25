use std::sync::Arc;

use byteorder::WriteBytesExt;
use id3::TagLike;
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
    pub fn format_all_media(&self) -> [String; P] {
        let mut out = [""; P].map(|s| s.to_owned());
        for i in 0..P {
            out[i] = self.playlists[i].format();
        }
        out
    }
    /// Format the ith media playlist
    pub fn format_media(&self, i: usize) -> Option<String> {
        Some(self.playlists.get(i)?.format())
    }
    /// Get the raw data of a segment from a media playlist with tags
    pub fn get_segment_raw(&self, playlist: usize, segment: usize) -> Option<Box<[u8]>> {
        self.playlists.get(playlist)?.get_segment_raw(segment)
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
        self.current += 1;
        self.segments[i] = segment;
    }
    /// Get the ith segment processed with tags
    pub fn get_segment_raw(&self, i: usize) -> Option<Box<[u8]>> {
        let index = self.current.checked_sub(i)?;
        if index > S {
            return None;
        };
        let mut seg = self.segments[if self.current_index >= index {
            self.current_index - index
        } else {
            self.current_index + S - index
        }]
        .get_raw()
        .into_vec();

        let timestamp = (i as u64) * 900000 * 10 * (1 + S as u64);
        let mut time_vec = Vec::new();
        time_vec
            .write_u64::<byteorder::BigEndian>(timestamp)
            .unwrap();
        let mut tag = id3::Tag::new();
        tag.add_frame(id3::frame::Private {
            owner_identifier: String::from("com.apple.streaming.transportStreamTimestamp"),
            private_data: time_vec,
        });
        let mut tag_vec = Vec::new();
        tag.write_to(&mut tag_vec, id3::Version::Id3v24)
            .expect("Couldn't write ID3");
        tag_vec.append(&mut seg);
        Some(tag_vec.into_boxed_slice())
    }
    /// Produce a formatted m3u8 String for the media playlist
    pub fn format(&self) -> String {
        // TODO: Confirm/Test this
        let start = if self.current >= S {
            self.current - S + 1
        } else {
            0
        };
        let segment_descrs = (start..=self.current).map(|i| {
            format!(
                "#EXTINF:10.000,
{i}.mp3"
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
pub struct Segment {
    raw: Box<[u8]>,
}

impl Segment {
    pub fn get_raw(&self) -> Box<[u8]> {
        self.raw.clone()
    }
    pub const fn new(raw: Box<[u8]>) -> Self {
        Self { raw }
    }
}

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

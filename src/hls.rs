use std::sync::Arc;

use tokio::time::Instant;

use crate::AppState;

#[derive(Debug, Clone)]
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
    pub fn format_master(&self, base_path: &str, bandwidths: [usize; S]) -> String {
        // TODO: Confirm/Test this
        let playlist_descrs = (0..P).map(|i| {
            let bandwidth = bandwidths[i];
            format!(
                "#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"{bandwidth}\",NAME=\"{bandwidth}\",AUTOSELECT=YES,DEFAULT=YES
            #EXT-X-STREAM-INF:BANDWIDTH={bandwidth},CODECS=\"mp3\"
            {base_path}/{bandwidth}/playlist.m3u8"
            )
        });
        format!(
            "#EXTM3U
            {}",
            playlist_descrs
                .reduce(|a, e| format!("{a}\n{e}"))
                .unwrap_or(String::new())
        )
    }
    pub fn format_media(&self) -> [String; P] {
        let mut out = [""; P].map(|s| s.to_owned());
        for i in 0..P {
            out[i] = self.playlists[i].format();
        }
        out
    }
}

#[derive(Debug, Clone)]
struct MediaPlaylist<const S: usize> {
    current_index: usize,
    current: usize,
    segments: [Segment; S],
}

impl<const S: usize> MediaPlaylist<S> {
    const fn new(segments: [Segment; S]) -> Self {
        Self {
            current_index: S - 1,
            current: 0,
            segments,
        }
    }
    fn add_segment(&mut self, segment: Segment) {
        let i = if self.current_index < S - 1 {
            self.current_index + 1
        } else {
            0
        };
        self.segments[i] = segment;
    }
    fn format(&self) -> String {
        // TODO: Confirm/Test this
        let start = self.current - S;
        let segment_descrs = (0..S).map(|i| {
            format!(
                "#EXTINF:10.000
            {i}.acc"
            )
        });
        format!(
            "#EXTM3U
            #EXT-X-VERSION:3
            #EXT-X-TARGETDURATION:10
            #ID3-EQUIV-TDTG:2023-10-02T03:18:35
            #EXT-X-PLAYLIST-TYPE:EVENT
            #EXT-X-MEDIA-SEQUENCE:{start}
            {}",
            segment_descrs
                .reduce(|a, e| format!("{a}\n{e}"))
                .unwrap_or(String::new())
        )
    }
}

#[derive(Debug, Clone)]
pub struct Segment {}

/// Function to add the new segments and set the new current segment
pub async fn update<const S: usize>(_instant: (Instant, Vec<[Segment; S]>), _data: Arc<AppState>) {
    // TODO: Update the HLS data on to instant
    println!("{}µs", _instant.0.elapsed().as_micros())
}

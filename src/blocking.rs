use std::{
    collections::HashMap,
    fs::{create_dir, remove_dir_all},
    io::Cursor,
    path::PathBuf,
    time::Duration,
};

use fdk_aac::enc::{ChannelMode, EncodeInfo, EncoderParams};
use symphonia::{
    core::{
        audio::{Channels, RawSampleBuffer, SampleBuffer, Signal, SignalSpec},
        codecs::{CodecParameters, CodecType, Decoder, DecoderOptions, CODEC_TYPE_NULL},
        formats::{FormatOptions, FormatReader},
        io::MediaSourceStream,
        meta::MetadataOptions,
        probe::Hint,
    },
    default::get_codecs,
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
        ext: String,
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

fn decode_loop(mut format: Box<dyn FormatReader>, mut decoder: Box<dyn Decoder>, track_id: u32) {
    use symphonia::core::errors::Error;

    let mut pcm = vec![];
    let mut spec = None;
    // The decode loop.
    loop {
        // Get the next packet from the media format.
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::ResetRequired) => {
                // The track list has been changed. Re-examine it and create a new set of decoders,
                // then restart the decode loop. This is an advanced feature and it is not
                // unreasonable to consider this "the end." As of v0.5.0, the only usage of this is
                // for chained OGG physical streams.
                unimplemented!();
            }
            Err(err) => {
                if let Error::IoError(ref err) = err {
                    if err.kind() == std::io::ErrorKind::UnexpectedEof {
                        break;
                    }
                }
                // A unrecoverable error occurred, halt decoding.
                panic!("Error reading packet: {:?}", err);
            }
        };

        // Consume any new metadata that has been read since the last packet.
        while !format.metadata().is_latest() {
            // Pop the old head of the metadata queue.
            format.metadata().pop();

            // Consume the new metadata at the head of the metadata queue.
        }

        // If the packet does not belong to the selected track, skip over it.
        if packet.track_id() != track_id {
            continue;
        }

        // Decode the packet into audio samples.
        match decoder.decode(&packet) {
            Ok(decoded) => {
                // Consume the decoded audio samples (see below).

                spec = Some(decoded.spec().clone());
                let mut sample_buf = SampleBuffer::new(decoded.capacity() as u64, *decoded.spec());

                sample_buf.copy_interleaved_ref(decoded);

                pcm.extend_from_slice(sample_buf.samples());
            }
            Err(Error::IoError(_)) => {
                // The packet failed to decode due to an IO error, skip the packet.
                continue;
            }
            Err(Error::DecodeError(_)) => {
                // The packet failed to decode due to invalid data, skip the packet.
                continue;
            }
            Err(err) => {
                // An unrecoverable error occurred, halt decoding.
                panic!("B: {}", err);
            }
        }
    }
    let SignalSpec { rate, channels } = spec.unwrap();
    if channels.count() > 2 {
        todo!("Discard other channels");
    }
    let num_channels = channels.count().clamp(0, 2);
    let each_len = rate as usize * 10 * num_channels;
    for (i, mut part) in pcm.chunks(each_len).enumerate() {
        let encoder = fdk_aac::enc::Encoder::new(EncoderParams {
            bit_rate: fdk_aac::enc::BitRate::VbrVeryHigh,
            sample_rate: rate,
            transport: fdk_aac::enc::Transport::Raw,
            channels: if num_channels == 2 {
                ChannelMode::Stereo
            } else {
                ChannelMode::Mono
            },
        })
        .unwrap();
        let mut compressed = Vec::<u8>::with_capacity(200000);
        'encode: loop {
            let mut comp_part = [0; 10000];
            let EncodeInfo {
                input_consumed,
                output_size,
            } = encoder.encode(part, &mut comp_part).unwrap();
            part = &part[input_consumed..];
            compressed.extend_from_slice(&comp_part[..output_size]);
            if part.len() == 0 {
                break 'encode;
            }
        }
        eprintln!("Saving part {i} with {}B data", compressed.len());
        // TODO: save audio
    }
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
    // PANICKING: Since 10 != 0 and x - x / 10000 == x * 0.9999 >= 0 for Duration x which by Typedefinition is >= 0, this should never panic
    // TODO(optimize): if the above proof is correct, we can unwrap_unchecked (unsafe)
    let short_interval = interval
        .checked_sub(interval.checked_div(10000).unwrap())
        .unwrap();
    let codecs = get_codecs();
    let mut last = std::time::Instant::now();
    let _start = last.clone();
    let seg = Segment::new(Box::new(include_bytes!("segment2.mp3").clone()));
    loop {
        let mut recvd = true;
        // Check for messages
        'mesg_check: {
            match srx.try_recv() {
                Ok(msg) => match msg {
                    ToBlocking::Upload {
                        radio,
                        song,
                        mut ext,
                        data,
                    } => {
                        // TODO(blocking): enable returning errors to user
                        let Ok(()) = create_dir(root_dir.join(&radio).join(song.to_string()))
                        else {
                            eprintln!("Couldn't create dir for song {song} in radio {radio} with root {}!", root_dir.display());
                            break 'mesg_check;
                        };
                        // TODO(audio): save songs (batching)
                        // get extension hint
                        ext.retain(|c| c != '.');
                        let mut hint = Hint::new();
                        hint.with_extension(&ext);

                        let mss =
                            MediaSourceStream::new(Box::new(Cursor::new(data)), Default::default());

                        // Use the default options for metadata and format readers.
                        let meta_opts: MetadataOptions = Default::default();
                        let fmt_opts: FormatOptions = Default::default();

                        // Probe the media source.
                        let probed = match symphonia::default::get_probe()
                            .format(&hint, mss, &fmt_opts, &meta_opts)
                        {
                            Ok(probed) => probed,
                            Err(e) => {
                                eprintln!("Got data of unsupported codec. Err: {e}");
                                break 'mesg_check;
                            }
                        };

                        // Get the instantiated format reader.
                        let mut format = probed.format;

                        // Find the first audio track with a known (decodeable) codec.
                        let Some(track) = format
                            .tracks()
                            .iter()
                            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
                        else {
                            eprintln!("No supported audio track!");
                            break 'mesg_check;
                        };

                        // Use the default options for the decoder.
                        let dec_opts: DecoderOptions = Default::default();

                        // Create a decoder for the track.
                        let mut decoder = match symphonia::default::get_codecs()
                            .make(&track.codec_params, &dec_opts)
                        {
                            Ok(decoder) => decoder,
                            Err(e) => {
                                eprintln!("Unsupported codec (2)! Err: {e}");
                                break 'mesg_check;
                            }
                        };

                        // Store the track identifier, it will be used to filter packets.
                        let track_id = track.id;
                        std::thread::spawn(move || decode_loop(format, decoder, track_id));
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
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => recvd = false,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => return,
            }
        }
        // Check if interval has been reached
        let diff = last.elapsed();
        if diff > short_interval {
            // TODO: send/create next fragment
            last += interval;
            atx.send((
                last.clone().into(),
                vec![("test".to_string(), [seg.clone()])],
            ))
            .unwrap();
        } else {
            if !recvd {
                std::thread::sleep(Duration::from_micros(1));
            }
        }
    }
}

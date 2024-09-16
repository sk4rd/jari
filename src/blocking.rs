use std::{
    collections::HashMap,
    fs::{create_dir, remove_dir_all},
    io::Cursor,
    path::PathBuf,
    time::Duration,
};

use fdk_aac::enc::{ChannelMode, EncodeInfo, EncoderParams};
use rayon::iter::{ParallelBridge, ParallelIterator};
use symphonia::core::{
    audio::{SampleBuffer, SignalSpec},
    codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL},
    formats::{FormatOptions, FormatReader},
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
};
use tokio::time::Instant;

use crate::{hls::Segment, BANDWIDTHS, NUM_BANDWIDTHS};

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

fn decode_loop(
    mut format: Box<dyn FormatReader>,
    mut decoder: Box<dyn Decoder>,
    track_id: u32,
    path: PathBuf,
) {
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
    let total_secs = pcm.len() as f64 / (rate as f64 * num_channels as f64);
    let encoder = fdk_aac::enc::Encoder::new(EncoderParams {
        bit_rate: fdk_aac::enc::BitRate::VbrVeryHigh,
        sample_rate: rate,
        transport: fdk_aac::enc::Transport::Adts,
        channels: if num_channels == 2 {
            ChannelMode::Stereo
        } else {
            ChannelMode::Mono
        },
    })
    .unwrap();
    let encoder_info = encoder.info().unwrap();

    let samples_per_chunk = 2 * encoder_info.frameLength as usize;

    let mut buf: [u8; 1536] = [0; 1536];

    // This is necessary because otherwise the encoder would output two frames of silence
    encoder
        .encode(&pcm[0..samples_per_chunk.clamp(0, pcm.len())], &mut buf)
        .unwrap();
    encoder
        .encode(
            &pcm[samples_per_chunk.clamp(0, pcm.len())
                ..(samples_per_chunk * 2).clamp(0, pcm.len())],
            &mut buf,
        )
        .unwrap();
    for (i, part) in pcm.chunks(each_len).enumerate() {
        let mut compressed = Vec::<u8>::new();

        for chunk in part.chunks(samples_per_chunk) {
            let EncodeInfo {
                input_consumed: _,
                output_size,
            } = encoder.encode(chunk, &mut buf).unwrap();
            compressed.extend_from_slice(&buf[..output_size]);
        }

        // Save file
        let path = path.clone().join(format!("{i}.aac"));
        std::fs::write(path, compressed).unwrap();
    }
    std::fs::write(path.join("len"), total_secs.to_string()).unwrap();
}
/// The blocking thread, contains mainly audio processing
pub fn main(
    atx: tokio::sync::mpsc::UnboundedSender<(
        Instant,
        Vec<(String, [Segment; crate::NUM_BANDWIDTHS])>,
    )>,
    mut srx: tokio::sync::mpsc::UnboundedReceiver<ToBlocking>,
    interval: Duration,
    radios: HashMap<String, Vec<u8>>,
    root_dir: PathBuf,
) {
    // PANICKING: Since 10 != 0 and x - x / 10000 == x * 0.9999 >= 0 for Duration x which by Typedefinition is >= 0, this should never panic
    // TODO(optimize): if the above proof is correct, we can unwrap_unchecked (unsafe)
    let short_interval = interval
        .checked_sub(interval.checked_div(2).unwrap())
        .unwrap();
    let mut last = std::time::Instant::now();
    let _start = last.clone();
    let mut radios_new = HashMap::new();
    radios_new.extend(radios.into_iter().map(|(name, order)| {
        use fdk_aac::enc::*;
        (
            name.clone(),
            (
                order,
                BANDWIDTHS.map(|band| {
                    Encoder::new(EncoderParams {
                        bit_rate: BitRate::Cbr(band as u32),
                        sample_rate: 44100,
                        transport: fdk_aac::enc::Transport::Adts,
                        channels: ChannelMode::Stereo,
                    })
                    .unwrap()
                }),
                44100,
            ),
        )
    }));
    let mut radios = radios_new;
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
                        let path = root_dir.join(&radio).join(song.to_string());
                        let Ok(()) = create_dir(&path) else {
                            eprintln!("Couldn't create dir for song {song} in radio {radio} with root {}!", root_dir.display());
                            break 'mesg_check;
                        };
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
                        let format = probed.format;

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
                        let decoder = match symphonia::default::get_codecs()
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
                        std::thread::spawn(move || decode_loop(format, decoder, track_id, path));
                    }
                    ToBlocking::Order { radio, order } => {
                        let Some((order_lock, _, _)) = radios.get_mut(&radio) else {
                            eprintln!("Tried to set the order for non-existent radio {radio}!");
                            break 'mesg_check;
                        };
                        *order_lock = order;
                    }
                    ToBlocking::Remove { radio, song } => {
                        let Some((order_lock, _, _)) = radios.get_mut(&radio) else {
                            eprintln!("Tried to remove song from non-existent radio {radio}!");
                            break 'mesg_check;
                        };
                        order_lock.retain(|e| e != &song);
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
                        use fdk_aac::enc::*;
                        radios.insert(
                            radio.clone(),
                            (
                                vec![],
                                BANDWIDTHS.map(|band| {
                                    Encoder::new(EncoderParams {
                                        bit_rate: BitRate::Cbr(band as u32),
                                        sample_rate: 44100,
                                        transport: fdk_aac::enc::Transport::Adts,
                                        channels: ChannelMode::Stereo,
                                    })
                                    .unwrap()
                                }),
                                44100,
                            ),
                        );
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
            let time_s = _start.elapsed().as_secs_f64();
            let segments = radios
                .iter_mut()
                .par_bridge()
                .map(|(name, (order, encoders, sample_rate))| {
                    let name = name.clone();
                    let path = root_dir.join(&name);
                    let lens: Box<[(u8, f64)]> = order
                        .iter()
                        .filter_map(|song| {
                            std::fs::read_to_string(path.join(song.to_string()).join("len"))
                                .ok()
                                .and_then(|v| v.parse().map(|x| (*song, x)).ok())
                                .ok_or(())
                                .map_err(|_| {
                                    eprintln!("Couldn't get len for song {song} in radio {name}")
                                })
                                .ok()
                        })
                        .collect();
                    let total_len: f64 = lens.iter().map(|(_, len)| len).sum();
                    let time = time_s % total_len;
                    let Some((song, offset, len)) = lens
                        .into_iter()
                        .scan(0.0f64, |pre_len, (song, len)| {
                            *pre_len += len;
                            Some((song, *pre_len, len))
                        })
                        .find(|(_, offset, _)| *offset >= time)
                    else {
                        return (name, Default::default());
                    };
                    let time = time - (offset - len);
                    let path = root_dir.join(&name).join(song.to_string());
                    let seg = (time / 10.0) as usize;
                    let Ok(data) = std::fs::read(path.join(seg.to_string()).with_extension("aac"))
                    else {
                        eprintln!("Couldn't read song file {seg} of song {song} in radio {name}");
                        return (name, Default::default());
                    };
                    // eprintln!("Serving segment {seg} of song {song} in radio {name} len {secs}s");
                    let Ok(segs) = recode(data, encoders, sample_rate) else {
                        eprintln!(
                            "Recoding error for segment {seg} of song {song} in radio {name}"
                        );
                        return (name, Default::default());
                    };
                    return (name, segs);
                })
                .collect();
            last += interval;
            atx.send((last.clone().into(), segments)).unwrap();
        } else {
            if !recvd {
                std::thread::sleep(Duration::from_micros(1));
            }
        }
    }
}

pub enum RecodeError {
    DecodeError(fdk_aac::dec::DecoderError),
    EncodeError(fdk_aac::enc::EncoderError),
}
impl From<fdk_aac::dec::DecoderError> for RecodeError {
    fn from(value: fdk_aac::dec::DecoderError) -> Self {
        Self::DecodeError(value)
    }
}
impl From<fdk_aac::enc::EncoderError> for RecodeError {
    fn from(value: fdk_aac::enc::EncoderError) -> Self {
        Self::EncodeError(value)
    }
}

fn recode(
    data: Vec<u8>,
    encoders: &mut [fdk_aac::enc::Encoder; NUM_BANDWIDTHS],
    current_sample_rate: &mut u32,
) -> Result<[Segment; NUM_BANDWIDTHS], RecodeError> {
    use fdk_aac::dec::*;
    use fdk_aac::enc::*;
    let mut segs = [(); NUM_BANDWIDTHS].map(|_| vec![]);
    let mut decoder = Decoder::new(fdk_aac::dec::Transport::Adts);
    let consumed = decoder.fill(&data)?;
    let mut data = &data[consumed..];
    let mut frame = [0; 2048];
    decoder.decode_frame(&mut frame).unwrap();
    let frame_size = decoder.decoded_frame_size();
    let stream_info = decoder.stream_info();
    // eprintln!("setting up encoders");
    let sample_rate = stream_info.sampleRate as u32;
    if sample_rate != *current_sample_rate {
        *current_sample_rate = sample_rate;
        *encoders = BANDWIDTHS.map(|band| {
            Encoder::new(EncoderParams {
                bit_rate: BitRate::Cbr(band as u32),
                sample_rate,
                transport: fdk_aac::enc::Transport::Adts,
                channels: ChannelMode::Stereo,
            })
            .unwrap()
        });
        for encoder in encoders.iter() {
            let encoder_info = encoder.info().unwrap();

            let samples_per_chunk = 2 * encoder_info.frameLength as usize;

            let mut buf: [u8; 1536] = [0; 1536];

            // This is necessary because otherwise the encoder would output two frames of silence
            encoder
                .encode(&frame[0..samples_per_chunk.clamp(0, frame.len())], &mut buf)
                .unwrap();
            encoder
                .encode(
                    &frame[samples_per_chunk.clamp(0, frame.len())
                        ..(samples_per_chunk * 2).clamp(0, frame.len())],
                    &mut buf,
                )
                .unwrap();
        }
    }
    for (i, encoder) in encoders.iter().enumerate() {
        let mut buf: [u8; 1536] = [0; 1536];
        let EncodeInfo {
            input_consumed: _,
            output_size,
        } = encoder.encode(&frame[..frame_size], &mut buf)?;
        segs[i].extend_from_slice(&buf[..output_size]);
    }
    // dbg!(stream_info.sampleRate);
    // eprintln!("starting decode-encode loop");
    let mut samples = 0;
    loop {
        // TODO(audio): make decoding somehow work
        let mut frame = vec![0; frame_size];
        match decoder.decode_frame(&mut frame) {
            Err(DecoderError::NOT_ENOUGH_BITS) => {
                if data.len() == 0 {
                    break;
                }
                let consumed = decoder.fill(data)?;
                data = &data[consumed..];
                continue;
            }
            Err(e) => Err(e)?,
            Ok(()) => (),
        };
        samples += frame_size;
        for (i, encoder) in encoders.iter_mut().enumerate() {
            let mut buf: [u8; 1536] = [0; 1536];
            let EncodeInfo {
                input_consumed: _,
                output_size,
            } = encoder.encode(&frame, &mut buf)?;
            segs[i].extend_from_slice(&buf[..output_size]);
        }
    }
    let secs = samples as f64 / sample_rate as f64 / 2.0; // 2.0 is for stereo
    Ok(segs.map(|seg| Segment::new(seg.into_boxed_slice(), secs)))
}

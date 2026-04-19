//! V4L2 M2M エンコードテスト。
//!
//! テストパターン (YUV420) を生成してエンコードし、MP4 ファイルに保存する。

use std::fs;
use std::io::{Seek, SeekFrom, Write};
use std::num::NonZeroU32;
use std::sync::mpsc;
use std::time::Duration;

use shiguredo_mp4::boxes::{Avc1Box, AvccBox, SampleEntry, VisualSampleEntryFields};
use shiguredo_mp4::mux::{Mp4FileMuxer, MuxError, Sample};
use shiguredo_mp4::{TrackKind, Uint};
use shiguredo_v4l2::v4l2_m2m;
use shiguredo_v4l2::v4l2_m2m::{EncodeCallbackOutput, EncodeInput, EncoderConfig, H264Encoder};

const NAL_TYPE_SPS: u8 = 7;
const NAL_TYPE_PPS: u8 = 8;

#[derive(Debug)]
enum Error {
    Args(noargs::Error),
    V4l2(v4l2_m2m::Error),
    Mp4(MuxError),
    Io(std::io::Error),
    Message(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Args(e) => write!(f, "{e:?}"),
            Error::V4l2(e) => write!(f, "{e}"),
            Error::Mp4(e) => write!(f, "{e}"),
            Error::Io(e) => write!(f, "{e}"),
            Error::Message(s) => f.write_str(s),
        }
    }
}

impl From<noargs::Error> for Error {
    fn from(e: noargs::Error) -> Self {
        Error::Args(e)
    }
}

impl From<v4l2_m2m::Error> for Error {
    fn from(e: v4l2_m2m::Error) -> Self {
        Error::V4l2(e)
    }
}

impl From<MuxError> for Error {
    fn from(e: MuxError) -> Self {
        Error::Mp4(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

type Result<T> = std::result::Result<T, Error>;

struct OwnedEncodedFrame {
    data: Vec<u8>,
    is_keyframe: bool,
    timestamp_us: i64,
    value: u64,
}

struct Args {
    device: String,
    width: u32,
    height: u32,
    bitrate_kbps: u32,
    frames: u32,
    output: Option<String>,
}

fn parse_args() -> Result<Args> {
    let mut args = noargs::raw_args();
    args.metadata_mut().app_name = env!("CARGO_PKG_NAME");
    args.metadata_mut().app_description = "V4L2 M2M エンコードテスト";

    if noargs::VERSION_FLAG.take(&mut args).is_present() {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

    noargs::HELP_FLAG.take_help(&mut args);

    let device: String = noargs::opt("device")
        .doc("エンコーダーデバイスパス (デフォルト: /dev/video11)")
        .example("/dev/video11")
        .take(&mut args)
        .present_and_then(|o| Ok::<_, &str>(o.value().to_string()))?
        .unwrap_or_else(|| "/dev/video11".to_string());

    let width: u32 = noargs::opt("width")
        .doc("入力映像の幅 (デフォルト: 640)")
        .take(&mut args)
        .present_and_then(|o| o.value().parse::<u32>())?
        .unwrap_or(640);

    let height: u32 = noargs::opt("height")
        .doc("入力映像の高さ (デフォルト: 480)")
        .take(&mut args)
        .present_and_then(|o| o.value().parse::<u32>())?
        .unwrap_or(480);

    let bitrate_kbps: u32 = noargs::opt("bitrate")
        .doc("ビットレート kbps (デフォルト: 1000)")
        .take(&mut args)
        .present_and_then(|o| o.value().parse::<u32>())?
        .unwrap_or(1000);

    let frames: u32 = noargs::opt("frames")
        .doc("エンコードフレーム数 (デフォルト: 30)")
        .take(&mut args)
        .present_and_then(|o| o.value().parse::<u32>())?
        .unwrap_or(30);

    let output: Option<String> = noargs::opt("output")
        .doc("MP4 出力ファイルパス (省略時はファイル出力なし)")
        .example("output.mp4")
        .take(&mut args)
        .present_and_then(|o| Ok::<_, &str>(o.value().to_string()))?;

    if let Some(help) = args.finish()? {
        print!("{help}");
        std::process::exit(0);
    }

    Ok(Args {
        device,
        width,
        height,
        bitrate_kbps,
        frames,
        output,
    })
}

/// グレーの単色 YUV420 テストパターンを生成する。
fn generate_test_frame(width: u32, height: u32) -> Vec<u8> {
    let y_size = (width * height) as usize;
    let uv_size = ((width / 2) * (height / 2)) as usize;
    let mut frame = vec![0u8; y_size + uv_size * 2];

    // Y=128, U=128, V=128 (グレー)
    frame[..y_size].fill(128);
    frame[y_size..y_size + uv_size].fill(128);
    frame[y_size + uv_size..].fill(128);

    frame
}

/// Annex B ストリームから NAL unit を分割する。
/// start code を除いた NAL unit データのスライスを返す。
fn split_nal_units(data: &[u8]) -> Vec<&[u8]> {
    let mut positions = Vec::new();

    let mut i = 0;
    while i + 2 < data.len() {
        if data[i] == 0 && data[i + 1] == 0 {
            if i + 3 < data.len() && data[i + 2] == 0 && data[i + 3] == 1 {
                positions.push((i, i + 4));
                i += 4;
            } else if data[i + 2] == 1 {
                positions.push((i, i + 3));
                i += 3;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    let mut nals = Vec::new();
    for (idx, &(_, nal_start)) in positions.iter().enumerate() {
        let nal_end = if idx + 1 < positions.len() {
            positions[idx + 1].0
        } else {
            data.len()
        };
        if nal_end > nal_start {
            nals.push(&data[nal_start..nal_end]);
        }
    }

    nals
}

/// NAL unit を AVCC 形式 (4 byte length prefix) に変換する。
/// SPS/PPS は除外する。
fn nals_to_avcc(nals: &[&[u8]]) -> Vec<u8> {
    let mut avcc = Vec::new();
    for nal in nals {
        if nal.is_empty() {
            continue;
        }
        let nal_type = nal[0] & 0x1f;
        if nal_type == NAL_TYPE_SPS || nal_type == NAL_TYPE_PPS {
            continue;
        }
        let len = nal.len() as u32;
        avcc.extend_from_slice(&len.to_be_bytes());
        avcc.extend_from_slice(nal);
    }
    avcc
}

/// 最初のキーフレームから SPS/PPS を抽出して SampleEntry を構築する。
fn create_sample_entry(
    width: u32,
    height: u32,
    nals: &[&[u8]],
) -> std::result::Result<SampleEntry, Error> {
    let mut sps_list: Vec<Vec<u8>> = Vec::new();
    let mut pps_list: Vec<Vec<u8>> = Vec::new();

    for nal in nals {
        if nal.is_empty() {
            continue;
        }
        let nal_type = nal[0] & 0x1f;
        match nal_type {
            NAL_TYPE_SPS => sps_list.push(nal.to_vec()),
            NAL_TYPE_PPS => pps_list.push(nal.to_vec()),
            _ => {}
        }
    }

    if sps_list.is_empty() {
        return Err(Error::Message(
            "SPS NAL unit が見つかりませんでした".to_string(),
        ));
    }

    // SPS から profile/level を抽出する
    let sps = &sps_list[0];
    let (profile, compat, level) = if sps.len() >= 4 {
        (sps[1], sps[2], sps[3])
    } else {
        (66, 0, 30)
    };

    Ok(SampleEntry::Avc1(Avc1Box {
        visual: VisualSampleEntryFields {
            data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
            width: width as u16,
            height: height as u16,
            horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
            vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
            frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
            compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
            depth: VisualSampleEntryFields::DEFAULT_DEPTH,
        },
        avcc_box: AvccBox {
            avc_profile_indication: profile,
            profile_compatibility: compat,
            avc_level_indication: level,
            length_size_minus_one: Uint::new(3),
            sps_list,
            pps_list,
            chroma_format: (!matches!(profile, 66 | 77 | 88)).then(|| Uint::new(1)),
            bit_depth_luma_minus8: (!matches!(profile, 66 | 77 | 88)).then(|| Uint::new(0)),
            bit_depth_chroma_minus8: (!matches!(profile, 66 | 77 | 88)).then(|| Uint::new(0)),
            sps_ext_list: vec![],
        },
        unknown_boxes: vec![],
    }))
}

fn main() -> Result<()> {
    let args = parse_args()?;

    let bitrate_bps = args.bitrate_kbps * 1000;
    let config = EncoderConfig::new(args.width, args.height, bitrate_bps);
    let config = EncoderConfig {
        device_path: args.device,
        ..config
    };

    println!(
        "エンコーダー設定: {}x{}, {}kbps, {:?} profile, {:?}",
        config.width, config.height, args.bitrate_kbps, config.profile, config.level
    );

    let width = config.width;
    let height = config.height;
    let (encode_tx, encode_rx) = mpsc::channel::<std::result::Result<OwnedEncodedFrame, String>>();
    let mut encoder = H264Encoder::new(config, move |result| {
        let mapped = match result {
            Ok(EncodeCallbackOutput::Frame { frame, value }) => match frame.data() {
                Some(data) => Ok(OwnedEncodedFrame {
                    data: data.to_vec(),
                    is_keyframe: frame.is_keyframe(),
                    timestamp_us: frame.timestamp_us(),
                    value,
                }),
                None => Err("encoder output is DMABUF in this sample".to_string()),
            },
            Err(err) => Err(format!("{err}")),
        };
        let _ = encode_tx.send(mapped);
    })?;

    let test_frame = generate_test_frame(width, height);

    // MP4 muxer とファイルのセットアップ
    let mut mp4_state: Option<(fs::File, Mp4FileMuxer, u64)> = match &args.output {
        Some(path) => {
            let muxer = Mp4FileMuxer::new()?;
            let mut file = fs::File::create(path)?;
            let initial = muxer.initial_boxes_bytes();
            file.write_all(initial)?;
            let offset = initial.len() as u64;
            Some((file, muxer, offset))
        }
        None => None,
    };

    let timescale = NonZeroU32::new(30).expect("timescale は 0 以外");
    let mut total_bytes: u64 = 0;
    let mut first_frame = true;

    println!("エンコード開始 ({} フレーム) ...", args.frames);

    for i in 0..args.frames {
        let timestamp_us = i as i64 * 33333;
        encoder.encode(
            EncodeInput::Mmap(&test_frame),
            timestamp_us,
            false,
            i as u64,
        )?;
        let encoded = match encode_rx.recv_timeout(Duration::from_secs(1)) {
            Ok(result) => result.map_err(Error::Message)?,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return Err(Error::Message("encode callback timed out".to_string()));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(Error::Message(
                    "encode callback channel disconnected".to_string(),
                ));
            }
        };

        if encoded.value != i as u64 {
            return Err(Error::Message(format!(
                "value mismatch: expected {}, got {}",
                i, encoded.value
            )));
        }

        let keyframe_str = if encoded.is_keyframe {
            ", keyframe"
        } else {
            ""
        };
        println!(
            "  frame {:>3}: {:>5} bytes{}, timestamp={}us",
            i + 1,
            encoded.data.len(),
            keyframe_str,
            encoded.timestamp_us
        );

        total_bytes += encoded.data.len() as u64;

        if let Some((ref mut file, ref mut muxer, ref mut data_offset)) = mp4_state {
            let nals = split_nal_units(&encoded.data);
            let avcc_data = nals_to_avcc(&nals);

            let sample_entry = if first_frame {
                Some(create_sample_entry(width, height, &nals)?)
            } else {
                None
            };

            file.write_all(&avcc_data)?;

            let sample = Sample {
                track_kind: TrackKind::Video,
                sample_entry,
                keyframe: encoded.is_keyframe,
                timescale,
                duration: 1,
                data_offset: *data_offset,
                data_size: avcc_data.len(),
            };
            muxer.append_sample(&sample)?;

            *data_offset += avcc_data.len() as u64;
        }

        first_frame = false;
    }

    // MP4 ファイルをファイナライズする
    if let Some((mut file, mut muxer, _)) = mp4_state {
        let finalized = muxer.finalize()?;
        for (offset, bytes) in finalized.offset_and_bytes_pairs() {
            file.seek(SeekFrom::Start(offset))?;
            file.write_all(bytes)?;
        }
    }

    let avg_bytes = if args.frames > 0 {
        total_bytes / args.frames as u64
    } else {
        0
    };

    println!();
    println!(
        "エンコード完了: {} フレーム, 合計 {} bytes, 平均 {} bytes/frame",
        args.frames, total_bytes, avg_bytes
    );

    if let Some(ref path) = args.output {
        println!("出力ファイル: {path}");
    }

    Ok(())
}

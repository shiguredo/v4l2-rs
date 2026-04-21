//! libcamera + H264Encoder で MP4 保存サンプル。
//!
//! Raspberry Pi カメラから映像をキャプチャし、H.264 エンコードして MP4 に保存する。
//! libcamera の DMA-BUF を mmap してコピーし、V4L2 エンコーダーに渡す。

use std::fs;
use std::io::{Seek, SeekFrom, Write};
use std::num::NonZeroU32;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use shiguredo_libcamera::{
    CameraManager, ConfigStatus, FrameBufferAllocator, FrameStatus, PixelFormat, RequestStatus,
    Size, StreamRole,
};
use shiguredo_mp4::boxes::{Avc1Box, AvccBox, SampleEntry, VisualSampleEntryFields};
use shiguredo_mp4::mux::{Mp4FileMuxer, MuxError, Sample};
use shiguredo_mp4::{TrackKind, Uint};
use shiguredo_v4l2::v4l2_m2m::{EncodeCallbackOutput, EncodeInput, EncoderConfig, H264Encoder};

const NAL_TYPE_SPS: u8 = 7;
const NAL_TYPE_PPS: u8 = 8;

/// YU12 (= I420) の FOURCC。V4L2_PIX_FMT_YUV420 と同じ値。
const YU12_FOURCC: u32 = u32::from_le_bytes([b'Y', b'U', b'1', b'2']);

#[derive(Debug)]
enum Error {
    Args(noargs::Error),
    Libcamera(shiguredo_libcamera::Error),
    V4l2(shiguredo_v4l2::v4l2_m2m::Error),
    Mp4(MuxError),
    Io(std::io::Error),
    Message(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Args(e) => write!(f, "{e:?}"),
            Error::Libcamera(e) => write!(f, "{e}"),
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

impl From<shiguredo_libcamera::Error> for Error {
    fn from(e: shiguredo_libcamera::Error) -> Self {
        Error::Libcamera(e)
    }
}

impl From<shiguredo_v4l2::v4l2_m2m::Error> for Error {
    fn from(e: shiguredo_v4l2::v4l2_m2m::Error) -> Self {
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
    value: u64,
}

struct Args {
    duration: u64,
    width: u32,
    height: u32,
    bitrate_kbps: u32,
    output: String,
    encoder_device: String,
}

fn parse_args() -> Result<Args> {
    let mut args = noargs::raw_args();
    args.metadata_mut().app_name = env!("CARGO_PKG_NAME");
    args.metadata_mut().app_description = "libcamera + H264Encoder で MP4 保存サンプル";

    if noargs::VERSION_FLAG.take(&mut args).is_present() {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

    noargs::HELP_FLAG.take_help(&mut args);

    let duration: u64 = noargs::opt("duration")
        .doc("キャプチャ秒数 (デフォルト: 5)")
        .take(&mut args)
        .present_and_then(|o| o.value().parse::<u64>())?
        .unwrap_or(5);

    let width: u32 = noargs::opt("width")
        .doc("解像度幅 (デフォルト: 1280)")
        .take(&mut args)
        .present_and_then(|o| o.value().parse::<u32>())?
        .unwrap_or(1280);

    let height: u32 = noargs::opt("height")
        .doc("解像度高さ (デフォルト: 720)")
        .take(&mut args)
        .present_and_then(|o| o.value().parse::<u32>())?
        .unwrap_or(720);

    let bitrate_kbps: u32 = noargs::opt("bitrate")
        .doc("ビットレート kbps (デフォルト: 4000)")
        .take(&mut args)
        .present_and_then(|o| o.value().parse::<u32>())?
        .unwrap_or(4000);

    let output: String = noargs::opt("output")
        .doc("MP4 出力ファイルパス (デフォルト: output.mp4)")
        .example("output.mp4")
        .take(&mut args)
        .present_and_then(|o| Ok::<_, &str>(o.value().to_string()))?
        .unwrap_or_else(|| "output.mp4".to_string());

    let encoder_device: String = noargs::opt("encoder-device")
        .doc("V4L2 エンコーダーデバイスパス (デフォルト: /dev/video11)")
        .example("/dev/video11")
        .take(&mut args)
        .present_and_then(|o| Ok::<_, &str>(o.value().to_string()))?
        .unwrap_or_else(|| "/dev/video11".to_string());

    if let Some(help) = args.finish()? {
        print!("{help}");
        std::process::exit(0);
    }

    Ok(Args {
        duration,
        width,
        height,
        bitrate_kbps,
        output,
        encoder_device,
    })
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

    println!(
        "設定: {}x{}, {}kbps, {}秒, 出力: {}",
        args.width, args.height, args.bitrate_kbps, args.duration, args.output
    );

    // 1. CameraManager
    let manager = CameraManager::new()?;
    if manager.cameras_count() == 0 {
        return Err(Error::Message("カメラが見つかりません".to_string()));
    }
    println!("カメラ数: {}", manager.cameras_count());

    // 2. get_camera(0) → acquire
    let mut camera = manager.get_camera(0)?;
    println!("カメラ ID: {}", camera.id());
    camera.acquire()?;

    // 3. generate_configuration
    let mut config = camera.generate_configuration(&[StreamRole::VideoRecording])?;

    // 4. StreamConfiguration の設定
    {
        let mut sc = config.at(0)?;
        sc.set_pixel_format(PixelFormat::from_fourcc(YU12_FOURCC));
        sc.set_size(Size {
            width: args.width,
            height: args.height,
        });
    }

    // 5. config.validate() - Adjusted の場合は調整後の値を使う
    let status = config.validate()?;
    if status == ConfigStatus::Invalid {
        return Err(Error::Message("設定が無効です".to_string()));
    }

    let (actual_width, actual_height) = {
        let sc = config.at(0)?;
        let size = sc.size();
        (size.width, size.height)
    };

    if status == ConfigStatus::Adjusted {
        println!("設定が調整されました: {}x{}", actual_width, actual_height);
    }

    // 6. camera.configure
    camera.configure(&mut config)?;

    // stride は configure 後に確定する
    let stride = {
        let sc = config.at(0)?;
        sc.stride()
    };

    println!(
        "ストリーム設定: {}x{}, stride={}",
        actual_width, actual_height, stride
    );

    // 7. stream
    let stream = {
        let sc = config.at(0)?;
        sc.stream()
            .ok_or_else(|| Error::Message("ストリームが取得できません".to_string()))?
    };

    // 8. H264Encoder (V4L2_MEMORY_MMAP で入力、libcamera DMA-BUF は mmap コピーして渡す)
    let encoder_config = EncoderConfig {
        device_path: args.encoder_device.clone(),
        stride,
        ..EncoderConfig::new(actual_width, actual_height, args.bitrate_kbps * 1000)
    };
    let (encode_tx, encode_rx) = mpsc::channel::<std::result::Result<OwnedEncodedFrame, String>>();
    let mut encoder = H264Encoder::new(encoder_config, move |result| {
        let mapped = match result {
            Ok(EncodeCallbackOutput::Frame { frame, value }) => match frame.data() {
                Some(data) => Ok(OwnedEncodedFrame {
                    data: data.to_vec(),
                    is_keyframe: frame.is_keyframe(),
                    value,
                }),
                None => Err("encoder output is DMABUF in this sample".to_string()),
            },
            Err(err) => Err(format!("{err}")),
        };
        let _ = encode_tx.send(mapped);
    })?;
    println!("エンコーダー初期化完了: デバイス={}", args.encoder_device);

    // 9. FrameBufferAllocator
    let allocator = FrameBufferAllocator::new(&camera);
    let buffer_count = allocator.allocate(&stream)?;
    println!("バッファ数: {buffer_count}");

    // 10. channel: (cookie/buffer_index, fd, offset, length, timestamp_us, encode)
    // Startup フレームも channel に投入して再キューイングを確保する。
    // encode=false の場合はエンコードをスキップするが、リクエストは必ず再投入する。
    let (tx, rx) = mpsc::channel::<(u64, i32, u32, u32, u64, bool)>();
    let stream_cb = stream.clone();
    let tx_cb = tx;
    camera.on_request_completed(move |completed| {
        if completed.status() != RequestStatus::Complete {
            return;
        }
        if let Some(buffer) = completed.find_buffer(&stream_cb) {
            let meta = buffer.metadata();
            if let Some(plane) = buffer.plane(0) {
                let timestamp_us = meta.timestamp / 1000;
                let encode = meta.status == FrameStatus::Success;
                let _ = tx_cb.send((
                    completed.cookie(),
                    plane.fd,
                    plane.offset,
                    plane.length,
                    timestamp_us,
                    encode,
                ));
            }
        }
    });

    // 12. requests を生成する
    let mut requests = Vec::with_capacity(buffer_count);
    for i in 0..buffer_count {
        let buffer = allocator.get_buffer(&stream, i)?;
        let request = camera.create_request(i as u64)?;
        request.add_buffer(&stream, &buffer)?;
        requests.push(request);
    }

    // 13. camera.start → queue_request (Running 状態でのみ queueRequest 可能)
    camera.start()?;
    println!("キャプチャ開始 ({}秒間) ...", args.duration);
    for request in &requests {
        camera.queue_request(request)?;
    }

    // MP4 muxer とファイルのセットアップ
    let mut muxer = Mp4FileMuxer::new()?;
    let mut file = fs::File::create(&args.output)?;
    let initial = muxer.initial_boxes_bytes();
    file.write_all(initial)?;
    let mut data_offset = initial.len() as u64;

    let timescale = NonZeroU32::new(30).expect("timescale は 0 以外");
    let mut first_frame = true;
    let mut frame_count: u32 = 0;

    // 14. メインスレッドでエンコードループ
    // クロージャでラップして、エラーが起きても camera.stop() が確実に呼ばれるようにする
    let loop_result: Result<()> = (|| -> Result<()> {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(args.duration) {
            let (cookie, fd, offset, length, timestamp_us, encode) =
                match rx.recv_timeout(Duration::from_secs(1)) {
                    Ok(v) => v,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                };

            // Startup フレームはスキップ、Success フレームのみエンコードする
            if encode {
                // libcamera の DMA-BUF を mmap して I420 データをコピーする
                // bcm2835-codec は V4L2_MEMORY_DMABUF の INPUT をサポートしていないため
                let frame_data = unsafe {
                    let ptr = libc::mmap(
                        std::ptr::null_mut(),
                        length as usize,
                        libc::PROT_READ,
                        libc::MAP_SHARED,
                        fd,
                        offset as libc::off_t,
                    );
                    if ptr == libc::MAP_FAILED {
                        return Err(Error::Io(std::io::Error::last_os_error()));
                    }
                    let slice = std::slice::from_raw_parts(ptr as *const u8, length as usize);
                    let vec = slice.to_vec();
                    libc::munmap(ptr, length as usize);
                    vec
                };

                encoder.encode(
                    EncodeInput::Mmap(&mut |buf, _resolution, _value| {
                        let size = frame_data.len();
                        buf[..size].copy_from_slice(&frame_data);
                        Some(size)
                    }),
                    timestamp_us as i64,
                    false,
                    timestamp_us,
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

                if encoded.value != timestamp_us {
                    return Err(Error::Message(format!(
                        "encode value mismatch: expected {}, got {}",
                        timestamp_us, encoded.value
                    )));
                }

                let nals = split_nal_units(&encoded.data);
                let avcc_data = nals_to_avcc(&nals);

                let sample_entry = if first_frame {
                    Some(create_sample_entry(actual_width, actual_height, &nals)?)
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
                    data_offset,
                    data_size: avcc_data.len(),
                };
                muxer.append_sample(&sample)?;
                data_offset += avcc_data.len() as u64;

                first_frame = false;
                frame_count += 1;
            }

            // Startup/Success に関わらず常に再キューイングして継続キャプチャ
            let idx = cookie as usize;
            requests[idx].reuse();
            camera.queue_request(&requests[idx])?;
        }
        Ok(())
    })();

    // 15. camera.stop → release (ループエラーに関わらず確実に実行)
    camera.stop()?;
    camera.release()?;

    // ループエラーを伝播
    loop_result?;

    // 16. MP4 ファイルをファイナライズ
    let finalized = muxer.finalize()?;
    for (offset, bytes) in finalized.offset_and_bytes_pairs() {
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(bytes)?;
    }

    println!("エンコード完了: {frame_count} フレーム");
    println!("出力ファイル: {}", args.output);

    Ok(())
}

use std::collections::BTreeMap;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use shiguredo_v4l2::v4l2_m2m::{
    ConvertCallbackOutput, ConvertInput, ConvertedFrame, ConverterConfig, Crop, DecodeInput,
    DecodedFrame, DecoderConfig, EncodeInput, EncodedFrame, EncoderConfig, Error, FnDecodeHandler,
    FnEncodeHandler, H264Decoder, H264Encoder, ImageConverter, Memory, PixelFormat, Resolution,
};

const FRAME_COUNT: usize = 10;
const SOURCE_WIDTH: u32 = 1920;
const SOURCE_HEIGHT: u32 = 1080;
const MIDDLE_WIDTH: u32 = 1280;
const MIDDLE_HEIGHT: u32 = 720;
const BITRATE_BPS: u32 = 4_000_000;
const FRAME_INTERVAL: Duration = Duration::from_millis(33);
const FLUSH_TIMEOUT: Duration = Duration::from_secs(20);
const MAE_THRESHOLD: f64 = 8.0;
const BUFFER_COUNT: u32 = 12;

#[derive(Debug, Clone, Copy)]
struct TestCase {
    // converter_in の出力メモリと encoder の入力メモリは一致させる。
    converter_in_output: Memory,
    encoder_input: Memory,
    // encoder の出力メモリと decoder の入力メモリは一致させる。
    encoder_output: Memory,
    decoder_input: Memory,
    // decoder の出力メモリと converter_out の入力メモリは一致させる。
    decoder_output: Memory,
    converter_out_input: Memory,
}

impl TestCase {
    fn all_mmap() -> Self {
        Self {
            converter_in_output: Memory::Mmap,
            encoder_input: Memory::Mmap,
            encoder_output: Memory::Mmap,
            decoder_input: Memory::Mmap,
            decoder_output: Memory::Mmap,
            converter_out_input: Memory::Mmap,
        }
    }

    fn mixed_dmabuf() -> Self {
        Self {
            converter_in_output: Memory::DmaBuf,
            encoder_input: Memory::DmaBuf,
            encoder_output: Memory::DmaBuf,
            decoder_input: Memory::DmaBuf,
            decoder_output: Memory::DmaBuf,
            converter_out_input: Memory::DmaBuf,
        }
    }
}

enum PipelineEvent {
    // 正常系は value と最終復元バッファだけをメインスレッドへ返す。
    Frame { value: i64, data: Vec<u8> },
    // 中間段を含むすべての失敗を 1 本化して返す。
    Error(String),
}

struct Pipeline {
    converter_in: ImageConverter<i64>,
    result_rx: Receiver<PipelineEvent>,
    restored_resolution: Resolution,
}

// encoder callback まで ConvertedFrame の寿命を保持する。
struct EncoderValue {
    value: i64,
    converted: ConvertedFrame,
}

// decoder callback まで EncodedFrame の寿命を保持する。
struct DecoderValue {
    value: i64,
    encoded: EncodedFrame<EncoderValue>,
}

// converter_out callback まで DecodedFrame の寿命を保持する。
struct ConverterOutValue {
    value: i64,
    decoded: DecodedFrame<DecoderValue>,
}

// 全て MMAP を利用してパイプラインを構築するケース
#[test]
fn test_converter_pipeline_all_mmap() {
    run_pipeline_test(TestCase::all_mmap());
}

// 全て DMABUF を利用してパイプラインを構築するケース
#[test]
fn test_converter_pipeline_mixed_dmabuf() {
    run_pipeline_test(TestCase::mixed_dmabuf());
}

// H264Encoder, H264Decoder, ImageConverter を使ったパイプラインを組んで、ラウンドトリップのテストを行う。
//
// I420 1080p の入力
// -> ImageConverter で NV12 720p に変換
// -> H264Encoder で H264 データに変換
// -> H264Decoder で再び I420 720p に変換
// -> ImageConveter で I420 1080p に変換
// -> 入力データと比較
//
// 10 フレーム分の入力を約 30 FPS で投入し、途中では同期待ちをせずに最後に一括で比較する
fn run_pipeline_test(case: TestCase) {
    let mut pipeline = build_pipeline(case);
    // value (= frame_no) をキーにし、入力と最終出力を突き合わせる。
    let mut expected = BTreeMap::<i64, Vec<u8>>::new();
    let mut restored = BTreeMap::<i64, Vec<u8>>::new();

    // 投入
    for frame_no in 0..FRAME_COUNT {
        let source = generate_i420_frame(SOURCE_WIDTH, SOURCE_HEIGHT, frame_no as u32);
        let value = frame_no as i64;
        let timestamp_us = frame_no as i64 * 33_333;
        expected.insert(value, source.clone());

        pipeline
            .converter_in
            .convert(
                ConvertInput::Mmap(&mut |buf, _resolution, _value| {
                    let size = source.len();
                    buf[..size].copy_from_slice(&source);
                    Some(size)
                }),
                timestamp_us,
                value,
            )
            .expect("converter_in の enqueue に失敗しました");

        // 入力レートを固定して、コールバックスレッド側の非同期処理を進める。
        thread::sleep(FRAME_INTERVAL);
    }

    // 全てのフレームの出力を待つ
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while restored.len() < FRAME_COUNT {
        let now = Instant::now();
        if now > deadline {
            panic!(
                "flush timeout: restored={}, expected={}",
                restored.len(),
                FRAME_COUNT
            );
        }
        let timeout = deadline.saturating_duration_since(now);
        match pipeline.result_rx.recv_timeout(timeout) {
            Ok(PipelineEvent::Frame { value, data }) => {
                // value をキーにして入力フレームと後で突き合わせる。
                restored.insert(value, data);
            }
            Ok(PipelineEvent::Error(err)) => panic!("pipeline callback エラー: {err}"),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                panic!(
                    "flush timeout: restored={}, expected={}",
                    restored.len(),
                    FRAME_COUNT
                );
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                panic!("pipeline callback channel が切断されました");
            }
        }
    }

    assert_eq!(
        restored.len(),
        FRAME_COUNT,
        "復元フレーム数が不足しています"
    );

    // フレームの比較
    for (value, original) in expected {
        let restored = restored
            .remove(&value)
            .expect("対応する復元フレームが見つかりません");

        let mae = y_plane_mae(
            &original,
            SOURCE_WIDTH,
            &restored,
            pipeline.restored_resolution.stride,
            SOURCE_WIDTH,
            SOURCE_HEIGHT,
        );
        assert!(
            mae <= MAE_THRESHOLD,
            "frame={value} の Y 面 MAE が閾値超過です: mae={mae:.3}, threshold={MAE_THRESHOLD:.3}"
        );
    }
}

// 入力 crop 機能のテスト。
//
// `ConverterConfig::crop` を指定した変換器で入力映像の一部が出力解像度へ拡大縮小されることを
// 検証する。実機の V4L2 M2M デバイス (/dev/video12) が必要なため、デバイスが無い環境では失敗する。
#[test]
fn test_converter_crop() {
    const CROP_WIDTH: u32 = 960;
    const CROP_HEIGHT: u32 = 540;

    // crop: None (フルフレーム) の変換器で基準出力を取得する。
    let mut full = make_crop_target(None).expect("crop: None の変換器の初期化に失敗しました");
    let full_out = convert_frame(&mut full, 1);

    // crop (0,0,960,540) を ConverterConfig で指定した変換器で出力を取得する。
    // crop は new() 内で STREAMON 前に S_SELECTION により適用される。
    let mut cropped = make_crop_target(Some(Crop {
        x: 0,
        y: 0,
        width: CROP_WIDTH,
        height: CROP_HEIGHT,
    }))
    .expect("crop 付き変換器の初期化に失敗しました");
    let crop_out = convert_frame(&mut cropped, 1);

    // crop が反映され、フルフレーム出力と内容が異なることを確認する。
    assert_ne!(
        full_out, crop_out,
        "crop が出力に反映されていません (フルフレームと同一出力です)"
    );

    // オフセット付き crop は bcm2835-codec が (0,0) 起点のみサポートするため反映されない。
    // G_SELECTION 検証により new() がエラーを返すことを確認する。
    let err = match make_crop_target(Some(Crop {
        x: 100,
        y: 100,
        width: CROP_WIDTH,
        height: CROP_HEIGHT,
    })) {
        Ok(_) => panic!("オフセット付き crop で new() が成功しました"),
        Err(err) => err,
    };
    assert!(
        matches!(err, Error::CropNotApplied { .. }),
        "オフセット付き crop が CropNotApplied で検知されませんでした: {err:?}"
    );
}

// crop 検証用の変換器。MMAP 入力 (I420) -> MMAP 出力 (NV12) で、出力解像度へ拡大縮小する。
// `crop` は ConverterConfig::crop に設定される。
struct CropTarget {
    converter: ImageConverter<i64>,
    rx: mpsc::Receiver<Result<(i64, Vec<u8>), String>>,
}

fn make_crop_target(crop: Option<Crop>) -> Result<CropTarget, Error> {
    let (tx, rx) = mpsc::channel::<Result<(i64, Vec<u8>), String>>();
    let mut config = ConverterConfig::new(SOURCE_WIDTH, SOURCE_HEIGHT, MIDDLE_WIDTH, MIDDLE_HEIGHT);
    config.input_pixel_format = PixelFormat::Yuv420;
    config.output_pixel_format = PixelFormat::Nv12;
    config.input_memory = Memory::Mmap;
    config.output_memory = Memory::Mmap;
    config.buffer_count = BUFFER_COUNT;
    config.crop = crop;
    let converter = ImageConverter::<i64>::new(config, move |result| {
        let msg = match result {
            Ok(ConvertCallbackOutput::Frame { frame, value }) => match frame.data() {
                Some(data) => Ok((value, data.to_vec())),
                None => Err("変換器が MMAP 出力を返しませんでした".to_string()),
            },
            Err(err) => Err(err.to_string()),
        };
        let _ = tx.send(msg);
    })?;
    Ok(CropTarget { converter, rx })
}

// テスト用の I420 フレームを変換器へ投入し、出力を取得する。
fn convert_frame(target: &mut CropTarget, seed: u32) -> Vec<u8> {
    let frame = generate_i420_frame(SOURCE_WIDTH, SOURCE_HEIGHT, seed);
    let frame_len = frame.len();
    target
        .converter
        .convert(
            ConvertInput::Mmap(&mut |buf, _res, _v| {
                buf[..frame_len].copy_from_slice(&frame);
                Some(frame_len)
            }),
            seed as i64,
            0,
        )
        .expect("変換に失敗しました");
    match target.rx.recv_timeout(FLUSH_TIMEOUT) {
        Ok(Ok((_, data))) => data,
        Ok(Err(err)) => panic!("変換のコールバックでエラーが発生しました: {err}"),
        Err(_) => panic!("変換の出力がタイムアウトしました"),
    }
}

fn build_pipeline(case: TestCase) -> Pipeline {
    // コールバックスレッド -> メインスレッドの最終通知チャネル。
    let (result_tx, result_rx) = mpsc::channel::<PipelineEvent>();

    // Stage 4: I420 720p -> I420 1080p
    let mut converter_out_config =
        ConverterConfig::new(MIDDLE_WIDTH, MIDDLE_HEIGHT, SOURCE_WIDTH, SOURCE_HEIGHT);
    converter_out_config.input_pixel_format = PixelFormat::Yuv420;
    converter_out_config.output_pixel_format = PixelFormat::Yuv420;
    converter_out_config.input_memory = case.converter_out_input;
    converter_out_config.output_memory = Memory::Mmap;
    converter_out_config.buffer_count = BUFFER_COUNT;

    let result_tx_converter_out = result_tx.clone();
    let converter_out =
        ImageConverter::<ConverterOutValue>::new(converter_out_config, move |result| {
            match result {
                Ok(ConvertCallbackOutput::Frame { frame, value }) => {
                    // value 側に保持していた DecodedFrame はここで drop され、requeue される。
                    let frame_no = value.value;
                    let data = match frame.data() {
                        Some(data) => data.to_vec(),
                        None => {
                            let _ = result_tx_converter_out.send(PipelineEvent::Error(
                                "converter_out は MMAP 出力である必要があります".to_string(),
                            ));
                            return;
                        }
                    };
                    let _ = result_tx_converter_out.send(PipelineEvent::Frame {
                        value: frame_no,
                        data,
                    });
                }
                Err(err) => {
                    let _ = result_tx_converter_out.send(PipelineEvent::Error(format!(
                        "converter_out callback エラー: {err}"
                    )));
                }
            }
        })
        .expect("converter_out の初期化に失敗しました");

    let restored_resolution = converter_out.output_resolution();

    // Stage 3: H264 -> I420 720p
    let mut decoder_config = DecoderConfig::new();
    decoder_config.input_memory = case.decoder_input;
    decoder_config.output_memory = case.decoder_output;
    decoder_config.output_buffer_count = BUFFER_COUNT;
    decoder_config.capture_buffer_count = BUFFER_COUNT;

    let result_tx_decoder = result_tx.clone();
    // 後段コンポーネントを callback が所有し、同一スレッド上で直列に中継する。
    let mut converter_out = converter_out;
    let converter_out_input = case.converter_out_input;
    let decoder = H264Decoder::new(
        decoder_config,
        FnDecodeHandler::new(
            move |result| match result {
                Ok(frame) => {
                    if let Err(err) = forward_decoded_to_converter_out(
                        &mut converter_out,
                        converter_out_input,
                        frame,
                    ) {
                        let _ = result_tx_decoder.send(PipelineEvent::Error(format!(
                            "decoder から converter_out への転送に失敗しました: {err}"
                        )));
                    }
                }
                Err(err) => {
                    let _ = result_tx_decoder.send(PipelineEvent::Error(format!(
                        "decoder callback エラー: {err}"
                    )));
                }
            },
            |_resolution| {
                // 本テストでは通知を受理するだけで追加処理はしない。
            },
        ),
    )
    .expect("decoder の初期化に失敗しました");

    // Stage 2: NV12 720p -> H264
    let mut encoder_config = EncoderConfig::new(MIDDLE_WIDTH, MIDDLE_HEIGHT, BITRATE_BPS);
    encoder_config.pixel_format = PixelFormat::Nv12;
    encoder_config.input_memory = case.encoder_input;
    encoder_config.output_memory = case.encoder_output;
    encoder_config.output_buffer_count = BUFFER_COUNT;
    encoder_config.capture_buffer_count = BUFFER_COUNT;

    let result_tx_encoder = result_tx.clone();
    // decoder も同様に callback が所有する。
    let mut decoder = decoder;
    let decoder_input = case.decoder_input;
    let encoder = H264Encoder::new(
        encoder_config,
        FnEncodeHandler::new(move |result| match result {
            Ok(frame) => {
                if let Err(err) = forward_encoded_to_decoder(&mut decoder, decoder_input, frame) {
                    let _ = result_tx_encoder.send(PipelineEvent::Error(format!(
                        "encoder から decoder への転送に失敗しました: {err}"
                    )));
                }
            }
            Err(err) => {
                let _ = result_tx_encoder.send(PipelineEvent::Error(format!(
                    "encoder callback エラー: {err}"
                )));
            }
        }),
    )
    .expect("encoder の初期化に失敗しました");

    // Stage 1: I420 1080p -> NV12 720p
    let mut converter_in_config =
        ConverterConfig::new(SOURCE_WIDTH, SOURCE_HEIGHT, MIDDLE_WIDTH, MIDDLE_HEIGHT);
    converter_in_config.input_pixel_format = PixelFormat::Yuv420;
    converter_in_config.output_pixel_format = PixelFormat::Nv12;
    converter_in_config.input_memory = Memory::Mmap;
    converter_in_config.output_memory = case.converter_in_output;
    converter_in_config.buffer_count = BUFFER_COUNT;

    let result_tx_converter_in = result_tx;
    // 最上流 callback から順に下流へ直接フォワードする。
    let mut encoder = encoder;
    let encoder_input = case.encoder_input;
    let converter_in = ImageConverter::new(converter_in_config, move |result| match result {
        Ok(ConvertCallbackOutput::Frame { frame, value }) => {
            if let Err(err) =
                forward_converted_to_encoder(&mut encoder, encoder_input, frame, value)
            {
                let _ = result_tx_converter_in.send(PipelineEvent::Error(format!(
                    "converter_in から encoder への転送に失敗しました: {err}"
                )));
            }
        }
        Err(err) => {
            let _ = result_tx_converter_in.send(PipelineEvent::Error(format!(
                "converter_in callback エラー: {err}"
            )));
        }
    })
    .expect("converter_in の初期化に失敗しました");

    Pipeline {
        converter_in,
        result_rx,
        restored_resolution,
    }
}

fn forward_converted_to_encoder(
    encoder: &mut H264Encoder<FnEncodeHandler<EncoderValue>>,
    encoder_input: Memory,
    frame: ConvertedFrame,
    value: i64,
) -> Result<(), String> {
    // 1 フレーム目だけキーフレームを強制し、以降は通常の P フレームにする。
    let force_keyframe = value == 0;
    let timestamp_us = frame.timestamp_us();

    match encoder_input {
        Memory::Mmap => {
            // MMAP 入力では callback で得たデータを encoder の入力バッファへコピーする。
            let next_value = EncoderValue {
                value,
                converted: frame,
            };
            encoder
                .encode(
                    EncodeInput::Mmap(&mut |buf, _resolution, user_data| {
                        let src = user_data
                            .converted
                            .data()
                            .expect("converter_in が MMAP でないデータを返しました");
                        let src_len = src.len();
                        buf[..src_len].copy_from_slice(src);
                        Some(src_len)
                    }),
                    timestamp_us,
                    force_keyframe,
                    next_value,
                )
                .map_err(|err| format!("encoder への MMAP 入力に失敗しました: {err}"))?;
        }
        Memory::DmaBuf => {
            // DMABUF 入力では FD と plane 情報をそのまま次段へ渡す。
            let fd = frame.dmabuf_fd().ok_or_else(|| {
                "encoder が DMABUF 入力のとき converter_in は DMABUF を返す必要があります"
                    .to_string()
            })?;
            let bytesused = frame.bytesused();
            let length = frame.length();
            let next_value = EncoderValue {
                value,
                converted: frame,
            };
            encoder
                .encode(
                    EncodeInput::DmaBuf {
                        fd,
                        bytesused,
                        length,
                    },
                    timestamp_us,
                    force_keyframe,
                    next_value,
                )
                .map_err(|err| format!("encoder への DMABUF 入力に失敗しました: {err}"))?;
        }
    }

    Ok(())
}

fn forward_encoded_to_decoder(
    decoder: &mut H264Decoder<FnDecodeHandler<DecoderValue>>,
    decoder_input: Memory,
    frame: EncodedFrame<EncoderValue>,
) -> Result<(), String> {
    // encoder 出力のタイムスタンプをそのまま decoder 入力へ伝播する。
    let timestamp_us = frame.timestamp_us();
    let frame_no = frame.user_data().value;

    match decoder_input {
        Memory::Mmap => {
            // MMAP 経路はコピー転送。
            let next_value = DecoderValue {
                value: frame_no,
                encoded: frame,
            };
            decoder
                .decode(
                    DecodeInput::Mmap(&mut |buf, user_data| {
                        let src = user_data
                            .encoded
                            .data()
                            .expect("encoder が MMAP でないデータを返しました");
                        let src_len = src.len();
                        buf[..src_len].copy_from_slice(src);
                        Some(src_len)
                    }),
                    timestamp_us,
                    next_value,
                )
                .map_err(|err| format!("decoder への MMAP 入力に失敗しました: {err}"))?;
        }
        Memory::DmaBuf => {
            // DMABUF 経路はゼロコピー転送。
            let fd = frame.dmabuf_fd().ok_or_else(|| {
                "decoder が DMABUF 入力のとき encoder は DMABUF を返す必要があります".to_string()
            })?;
            let bytesused = frame.bytesused();
            let length = frame.length();
            let next_value = DecoderValue {
                value: frame_no,
                encoded: frame,
            };
            decoder
                .decode(
                    DecodeInput::DmaBuf {
                        fd,
                        bytesused,
                        length,
                    },
                    timestamp_us,
                    next_value,
                )
                .map_err(|err| format!("decoder への DMABUF 入力に失敗しました: {err}"))?;
        }
    }

    Ok(())
}

fn forward_decoded_to_converter_out(
    converter_out: &mut ImageConverter<ConverterOutValue>,
    converter_out_input: Memory,
    frame: DecodedFrame<DecoderValue>,
) -> Result<(), String> {
    // decoder 出力を最終 converter へ受け渡す。
    let timestamp_us = frame.timestamp_us();
    let frame_no = frame.user_data().value;

    match converter_out_input {
        Memory::Mmap => {
            // MMAP 経路はコピー転送。
            let next_value = ConverterOutValue {
                value: frame_no,
                decoded: frame,
            };
            converter_out
                .convert(
                    ConvertInput::Mmap(&mut |buf, _resolution, user_data| {
                        let src = user_data
                            .decoded
                            .data()
                            .expect("decoder が MMAP でないデータを返しました");
                        let src_len = src.len();
                        buf[..src_len].copy_from_slice(src);
                        Some(src_len)
                    }),
                    timestamp_us,
                    next_value,
                )
                .map_err(|err| format!("converter_out への MMAP 入力に失敗しました: {err}"))?;
        }
        Memory::DmaBuf => {
            // DMABUF 経路はゼロコピー転送。
            let fd = frame.dmabuf_fd().ok_or_else(|| {
                "converter_out が DMABUF 入力のとき decoder は DMABUF を返す必要があります"
                    .to_string()
            })?;
            let bytesused = frame.bytesused();
            let length = frame.length();
            let next_value = ConverterOutValue {
                value: frame_no,
                decoded: frame,
            };
            converter_out
                .convert(
                    ConvertInput::DmaBuf {
                        fd,
                        bytesused,
                        length,
                    },
                    timestamp_us,
                    next_value,
                )
                .map_err(|err| format!("converter_out への DMABUF 入力に失敗しました: {err}"))?;
        }
    }

    Ok(())
}

fn generate_i420_frame(width: u32, height: u32, frame_no: u32) -> Vec<u8> {
    // フレーム番号で変化するパターンを作り、取り違えや劣化を検出しやすくする。
    let y_stride = width as usize;
    let y_height = height as usize;
    let y_size = y_stride * y_height;
    let uv_stride = y_stride / 2;
    let uv_height = y_height / 2;
    let uv_size = uv_stride * uv_height;

    let mut frame = vec![0_u8; y_size + uv_size * 2];

    for y in 0..y_height {
        let row_offset = y * y_stride;
        for x in 0..y_stride {
            let luma = 16 + ((x / 4 + y / 4 + frame_no as usize * 3) % 220);
            frame[row_offset + x] = luma as u8;
        }
    }

    let u_offset = y_size;
    let v_offset = y_size + uv_size;
    for y in 0..uv_height {
        for x in 0..uv_stride {
            let u = 96 + ((x / 4 + y / 6 + frame_no as usize * 2) % 64);
            let v = 96 + ((x / 6 + y / 4 + frame_no as usize * 2) % 64);
            frame[u_offset + y * uv_stride + x] = u as u8;
            frame[v_offset + y * uv_stride + x] = v as u8;
        }
    }

    frame
}

fn y_plane_mae(
    lhs: &[u8],
    lhs_stride: u32,
    rhs: &[u8],
    rhs_stride: u32,
    width: u32,
    height: u32,
) -> f64 {
    // stride を考慮して Y 面のみ MAE を計算する。
    let lhs_stride = lhs_stride as usize;
    let rhs_stride = rhs_stride as usize;
    let width = width as usize;
    let height = height as usize;

    let mut sum_abs_diff: u64 = 0;
    for y in 0..height {
        let lhs_row = y * lhs_stride;
        let rhs_row = y * rhs_stride;
        for x in 0..width {
            let l = lhs[lhs_row + x];
            let r = rhs[rhs_row + x];
            sum_abs_diff += l.abs_diff(r) as u64;
        }
    }

    sum_abs_diff as f64 / (width * height) as f64
}

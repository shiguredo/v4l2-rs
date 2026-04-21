//! V4L2 M2M デバイスのチェック。
//!
//! エンコーダー/デコーダーデバイスの存在確認、権限確認、初期化確認を行う。

use std::fs;

use shiguredo_v4l2::v4l2_m2m;
use shiguredo_v4l2::v4l2_m2m::{DecoderConfig, EncoderConfig, H264Decoder, H264Encoder};

#[derive(Debug)]
enum Error {
    Args(noargs::Error),
    V4l2(v4l2_m2m::Error),
    Io(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Args(e) => write!(f, "{e:?}"),
            Error::V4l2(e) => write!(f, "{e}"),
            Error::Io(e) => write!(f, "{e}"),
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

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

type Result<T> = std::result::Result<T, Error>;

struct Args {
    encoder_device: String,
    decoder_device: String,
}

fn parse_args() -> Result<Args> {
    let mut args = noargs::raw_args();
    args.metadata_mut().app_name = env!("CARGO_PKG_NAME");
    args.metadata_mut().app_description = "V4L2 M2M デバイスチェック";

    if noargs::VERSION_FLAG.take(&mut args).is_present() {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

    noargs::HELP_FLAG.take_help(&mut args);

    let encoder_device: String = noargs::opt("encoder-device")
        .doc("エンコーダーデバイスパス (デフォルト: /dev/video11)")
        .example("/dev/video11")
        .take(&mut args)
        .present_and_then(|o| Ok::<_, &str>(o.value().to_string()))?
        .unwrap_or_else(|| "/dev/video11".to_string());

    let decoder_device: String = noargs::opt("decoder-device")
        .doc("デコーダーデバイスパス (デフォルト: /dev/video10)")
        .example("/dev/video10")
        .take(&mut args)
        .present_and_then(|o| Ok::<_, &str>(o.value().to_string()))?
        .unwrap_or_else(|| "/dev/video10".to_string());

    if let Some(help) = args.finish()? {
        print!("{help}");
        std::process::exit(0);
    }

    Ok(Args {
        encoder_device,
        decoder_device,
    })
}

fn check_device_exists(label: &str, path: &str) -> bool {
    print!("[{label}] デバイスファイル確認: {path} ... ");
    match fs::metadata(path) {
        Ok(_) => {
            println!("OK");
            true
        }
        Err(e) => {
            println!("NG ({e})");
            false
        }
    }
}

fn check_device_permission(label: &str, path: &str) -> bool {
    print!("[{label}] 読み書き権限確認 ... ");
    match fs::OpenOptions::new().read(true).write(true).open(path) {
        Ok(_) => {
            println!("OK");
            true
        }
        Err(e) => {
            println!("NG ({e})");
            false
        }
    }
}

fn main() -> Result<()> {
    let args = parse_args()?;

    let mut ok = true;

    // エンコーダーチェック
    if check_device_exists("encoder", &args.encoder_device) {
        if check_device_permission("encoder", &args.encoder_device) {
            print!("[encoder] 初期化 (640x480, 1Mbps) ... ");
            let config = EncoderConfig::new(640, 480, 1_000_000);
            let config = EncoderConfig {
                device_path: args.encoder_device.clone(),
                ..config
            };
            match H264Encoder::<()>::new(config, |_| {}) {
                Ok(_) => println!("OK"),
                Err(e) => {
                    println!("NG ({e})");
                    ok = false;
                }
            }
        } else {
            ok = false;
        }
    } else {
        ok = false;
    }

    // デコーダーチェック
    if check_device_exists("decoder", &args.decoder_device) {
        if check_device_permission("decoder", &args.decoder_device) {
            print!("[decoder] 初期化 ... ");
            let config = DecoderConfig {
                device_path: args.decoder_device.clone(),
                ..DecoderConfig::new()
            };
            match H264Decoder::<()>::new(config, |_| {}) {
                Ok(_) => println!("OK"),
                Err(e) => {
                    println!("NG ({e})");
                    ok = false;
                }
            }
        } else {
            ok = false;
        }
    } else {
        ok = false;
    }

    println!();
    if ok {
        println!("全てのチェックに成功しました");
    } else {
        println!("一部のチェックに失敗しました");
        std::process::exit(1);
    }

    Ok(())
}

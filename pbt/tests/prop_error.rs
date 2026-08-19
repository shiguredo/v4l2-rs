use proptest::prelude::*;
use shiguredo_v4l2::v4l2_m2m::{Crop, Error};

fn arb_crop() -> impl Strategy<Value = Crop> {
    (any::<u32>(), any::<u32>(), any::<u32>(), any::<u32>()).prop_map(|(x, y, width, height)| {
        Crop {
            x,
            y,
            width,
            height,
        }
    })
}

fn arb_error() -> impl Strategy<Value = Error> {
    prop_oneof![
        any::<String>().prop_map(|path| Error::DeviceOpen {
            path,
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "test"),
        }),
        any::<String>().prop_map(|_| Error::Ioctl {
            request: "VIDIOC_S_FMT",
            source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "test"),
        }),
        any::<String>().prop_map(|_| Error::Mmap {
            source: std::io::Error::other("test"),
        }),
        any::<String>().prop_map(|_| Error::Poll {
            source: std::io::Error::new(std::io::ErrorKind::Interrupted, "test"),
        }),
        any::<String>().prop_map(|reason| Error::InvalidFormat { reason }),
        any::<String>().prop_map(|_| Error::NoAvailableBuffer),
        any::<String>().prop_map(|_| Error::NotStarted),
        any::<String>().prop_map(|_| Error::StreamOn {
            source: std::io::Error::other("test"),
        }),
        any::<String>().prop_map(|_| Error::StreamOff {
            source: std::io::Error::other("test"),
        }),
        (any::<usize>(), any::<usize>())
            .prop_map(|(size, capacity)| Error::InputTooLarge { size, capacity }),
        any::<String>().prop_map(|_| Error::MmapInputNotProduced),
        any::<String>().prop_map(|_| Error::PollerAborted),
        (arb_crop(), arb_crop())
            .prop_map(|(requested, actual)| Error::CropNotApplied { requested, actual }),
    ]
}

proptest! {
    /// 全エラーバリアントの Display は空文字列にならない。
    #[test]
    fn error_display_non_empty(err in arb_error()) {
        let msg = format!("{err}");
        prop_assert!(!msg.is_empty(), "Display が空文字列です: {err:?}");
    }

    /// 全エラーバリアントの Debug は空文字列にならない。
    #[test]
    fn error_debug_non_empty(err in arb_error()) {
        let msg = format!("{err:?}");
        prop_assert!(!msg.is_empty(), "Debug が空文字列です");
    }

    /// source() を持つバリアントは std::io::Error を返す。
    #[test]
    fn error_source_consistency(err in arb_error()) {
        use std::error::Error as StdError;
        // source() が Some の場合、必ず Display 可能であること
        if let Some(source) = err.source() {
            let msg = format!("{source}");
            prop_assert!(!msg.is_empty());
        }
    }
}

use proptest::prelude::*;
use shiguredo_v4l2::v4l2_m2m::{H264Level, H264Profile, PixelFormat, Resolution};

proptest! {
    /// PixelFormat の fourcc 往復変換が一致する。
    #[test]
    fn pixel_format_roundtrip(fmt in prop_oneof![
        Just(PixelFormat::Yuv420),
        Just(PixelFormat::Nv12),
        Just(PixelFormat::H264),
    ]) {
        let fourcc = fmt.to_fourcc();
        let restored = PixelFormat::from_fourcc(fourcc);
        prop_assert_eq!(restored, Some(fmt));
    }

    /// 未知の fourcc は None を返す。
    #[test]
    fn pixel_format_unknown_fourcc(fourcc in any::<u32>()) {
        let known = [
            PixelFormat::Yuv420.to_fourcc(),
            PixelFormat::Nv12.to_fourcc(),
            PixelFormat::H264.to_fourcc(),
        ];
        if !known.contains(&fourcc) {
            prop_assert_eq!(PixelFormat::from_fourcc(fourcc), None);
        }
    }

    /// H264Profile の V4L2 往復変換が一致する。
    #[test]
    fn h264_profile_roundtrip(profile in prop_oneof![
        Just(H264Profile::Baseline),
        Just(H264Profile::ConstrainedBaseline),
        Just(H264Profile::Main),
        Just(H264Profile::High),
    ]) {
        let v4l2_val = profile.to_v4l2();
        let restored = H264Profile::from_v4l2(v4l2_val);
        prop_assert_eq!(restored, Some(profile));
    }

    /// H264Level の V4L2 往復変換が一致する。
    #[test]
    fn h264_level_roundtrip(level in prop_oneof![
        Just(H264Level::Level3_0),
        Just(H264Level::Level3_1),
        Just(H264Level::Level3_2),
        Just(H264Level::Level4_0),
        Just(H264Level::Level4_1),
        Just(H264Level::Level4_2),
        Just(H264Level::Level5_0),
        Just(H264Level::Level5_1),
    ]) {
        let v4l2_val = level.to_v4l2();
        let restored = H264Level::from_v4l2(v4l2_val);
        prop_assert_eq!(restored, Some(level));
    }

    /// Resolution の yuv420_size は width * height * 3 / 2 に等しい。
    #[test]
    fn yuv420_size_formula(width in 2u32..8192, height in 2u32..8192) {
        // 偶数に正規化
        let width = width & !1;
        let height = height & !1;
        let res = Resolution { width, height };
        let expected = (width as usize) * (height as usize) * 3 / 2;
        prop_assert_eq!(res.yuv420_size(), expected);
    }

    /// stride 付き yuv420_size は stride >= width の場合 stride ベースで計算される。
    #[test]
    fn yuv420_size_with_stride(
        width in 2u32..4096,
        height in 2u32..4096,
        extra in 0u32..256,
    ) {
        let width = width & !1;
        let height = height & !1;
        let stride = width + (extra & !1);
        let res = Resolution { width, height };
        let expected = (stride as usize) * (height as usize)
            + (stride as usize / 2) * (height as usize / 2) * 2;
        prop_assert_eq!(res.yuv420_size_with_stride(stride), expected);
    }
}

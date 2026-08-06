use shiguredo_v4l2::v4l2_m2m::Resolution;

// yuv420_size は任意の width / height / stride が渡されてもパニックしない (境界値)。
// PBT では任意入力の組み合わせを網羅できないため、単体テストで上限値を検証する。
#[test]
fn yuv420_size_does_not_panic_on_extreme_values() {
    let res = Resolution {
        width: u32::MAX,
        height: u32::MAX,
        stride: u32::MAX,
    };
    let _ = res.yuv420_size();
}

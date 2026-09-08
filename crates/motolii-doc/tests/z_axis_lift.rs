//! レイヤー transform の Z(M5 F-1 の段階引き上げ)— **red 先行**。
//!
//! 前提(実測): [M5仕様](../../../docs/specs/M5-3d-and-post.md) F-1 は
//! 「2Dレイヤーは『Z=0のXY平面に置かれた高さ1.0のクワッド』として世界に置く」と
//! 定めており、世界は最初から3D、Stage も `rerun_stage/document_camera.rs` で
//! `COMPOSITION_PLANE_Z` を持つ。欠けているのは**レイヤーがその平面から離れる口**
//! だけである。`Transform2D`(`crates/motolii-doc/src/schema.rs:469`)は
//! position / anchor / scale / rotation / parent しか持たない。
//!
//! この試験が固定する契約は4本。**版を上げない**ことと、
//! **既存文書のバイト列を変えない**ことが中心である
//! (先例: locators と `Composition.resolution` — 空なら書き出さない)。

use motolii_doc::{DocParam, DocValue, Transform2D};

/// `DocParam` に `as_const_f64` は無いので、試験側で束ねる
/// (`param.rs` はこのレーンの ALLOWLIST 外 — 意図は一切変えていない)。
fn as_const_f64(param: &DocParam) -> Option<f64> {
    match param {
        DocParam::Const(DocValue::F64(v)) => Some(*v),
        _ => None,
    }
}

/// 1. 既定の transform は z=0 を**持つ**(全員が z=0 の世界に居る、を型で言う)。
#[test]
fn the_identity_transform_sits_at_z_zero() {
    let t = Transform2D::identity();
    let z = as_const_f64(&t.z).expect("z は Const(f64) の既定を持つ");
    assert_eq!(z, 0.0, "既定の z は 0.0(= z=0 平面)");
}

/// 2. z が既定のままなら **JSON に現れない**。旧文書とバイト列が変わらない。
#[test]
fn a_default_z_is_not_written_to_json() {
    let doc = motolii_doc::Document::new_v1();
    let json = serde_json::to_string_pretty(&doc).expect("serialize");
    assert!(
        !json.contains("\"z\""),
        "z が既定(0.0)のときは書き出さない — 旧 reader との往復を壊さないため"
    );
}

/// 3. 版は上がらない。**要素を1つ足しただけで版を上げない**。
#[test]
fn adding_z_does_not_raise_the_document_version() {
    let doc = motolii_doc::Document::new_v1();
    let before_version = doc.version;
    let before_floor = doc.min_reader_version;
    let json = serde_json::to_string(&doc).expect("serialize");
    let back: motolii_doc::Document = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.version, before_version, "version を上げない");
    assert_eq!(
        back.min_reader_version, before_floor,
        "min_reader_version を上げない"
    );
}

/// 4. z を持たない**旧 JSON** は z=0 として読める(前方互換)。
#[test]
fn old_json_without_z_reads_as_z_zero() {
    let doc = motolii_doc::Document::new_v1();
    let json = serde_json::to_string(&doc).expect("serialize");
    assert!(!json.contains("\"z\""), "前提: 既定は書き出されない");
    let back: motolii_doc::Document = serde_json::from_str(&json).expect("旧形状が読める");
    // 文書に layer が居ないので、identity の既定だけを見る。
    let t = Transform2D::identity();
    assert_eq!(
        as_const_f64(&t.z).expect("const f64"),
        0.0,
        "z を書いていない文書は z=0 として読む"
    );
    let _ = back;
}

/// 5. **非空虚性**(supervisor 追加)。1〜4 は `Document::new_v1()` に layer が
/// 1つも居らず `Transform2D` 自体が直列化されないため、`"z"` を含まないことが
/// **素通りで満たされてしまう**。発注側(=この試験を書いた側)の穴で、実装担当が
/// 一時 probe で検出して報告した。ここで恒久化する。
#[test]
fn a_non_default_z_is_written_and_round_trips() {
    let mut t = Transform2D::identity();
    let json_default = serde_json::to_string(&t).expect("serialize default");
    assert!(
        !json_default.contains("\"z\""),
        "既定(0.0)は書き出さない: {json_default}"
    );

    t.z = DocParam::const_f64(-3.5);
    let json = serde_json::to_string(&t).expect("serialize non-default");
    assert!(json.contains("\"z\""), "既定でない z は書き出す: {json}");

    let back: Transform2D = serde_json::from_str(&json).expect("round trip");
    assert_eq!(as_const_f64(&back.z), Some(-3.5), "z が往復する");
}

#![allow(deprecated)]

//! D1i-2: PathOp意味論表(docs/specs/M2-document-model.md)の拒否項目 + 追加席の serde 契約。
//! 幾何の意味論ゴールデンは `d1i2_pathop_geometry.rs`。

use motolii_core::{RationalTime, TimeMap};
use motolii_doc::{
    Clip, ClipSource, CompositeOrder, DocParam, Document, DocumentError, ItemEnvelope, LineJoin,
    PathOp, PointType, StandardShape, Track, TrackItem, TrimMode, VectorContent, VectorRecipe,
};
use serde_json::json;

fn doc_with_modifiers(modifiers: Vec<PathOp>) -> Document {
    let mut doc = Document::new_current();
    let layer = doc.layers.allocate("a").unwrap();
    let tid = doc.track_ids.allocate("V1").unwrap();
    doc.tracks.push(Track {
        id: tid,
        items: vec![TrackItem::Clip(Clip {
            envelope: ItemEnvelope::new(layer),
            start: RationalTime::ZERO,
            duration: RationalTime::try_new(5, 1).unwrap(),
            time_map: TimeMap::default(),
            source: ClipSource::Vector {
                recipe: VectorRecipe {
                    content: VectorContent::StandardShape {
                        shape: StandardShape::Rect {
                            width: DocParam::const_f64(1.0),
                            height: DocParam::const_f64(1.0),
                        },
                    },
                    modifiers,
                },
            },
        })],
    });
    doc
}

fn twist(angle: f64, center: [f64; 2]) -> PathOp {
    PathOp::Twist {
        angle: DocParam::const_f64(angle),
        center: DocParam::const_vec2(center),
    }
}

// --- pucker_bloat.amount ∈ [-1, 1] ---

#[test]
fn pucker_bloat_amount_in_range_ok() {
    let doc = doc_with_modifiers(vec![PathOp::PuckerBloat {
        amount: DocParam::const_f64(1.0),
    }]);
    assert!(doc.validate().is_ok());
    let doc = doc_with_modifiers(vec![PathOp::PuckerBloat {
        amount: DocParam::const_f64(-1.0),
    }]);
    assert!(doc.validate().is_ok());
}

#[test]
fn pucker_bloat_amount_out_of_range_rejected() {
    let doc = doc_with_modifiers(vec![PathOp::PuckerBloat {
        amount: DocParam::const_f64(1.5),
    }]);
    assert!(matches!(
        doc.validate(),
        Err(DocumentError::ValueOutOfRange { .. })
    ));
    let doc = doc_with_modifiers(vec![PathOp::PuckerBloat {
        amount: DocParam::const_f64(-1.0001),
    }]);
    assert!(matches!(
        doc.validate(),
        Err(DocumentError::ValueOutOfRange { .. })
    ));
}

// --- zig_zag.amount / ridges ≥ 0, point_type serde default ---

#[test]
fn zig_zag_negative_amount_rejected() {
    let doc = doc_with_modifiers(vec![PathOp::ZigZag {
        amount: DocParam::const_f64(-0.01),
        ridges: DocParam::const_f64(3.0),
        point_type: PointType::Corner,
    }]);
    assert!(matches!(
        doc.validate(),
        Err(DocumentError::ValueOutOfRange { .. })
    ));
}

#[test]
fn zig_zag_negative_ridges_rejected() {
    let doc = doc_with_modifiers(vec![PathOp::ZigZag {
        amount: DocParam::const_f64(0.05),
        ridges: DocParam::const_f64(-1.0),
        point_type: PointType::Smooth,
    }]);
    assert!(matches!(
        doc.validate(),
        Err(DocumentError::ValueOutOfRange { .. })
    ));
}

#[test]
fn zig_zag_point_type_defaults_to_corner_on_legacy_json() {
    let json = json!({
        "op": "zig_zag",
        "amount": {"const": {"F64": 0.05}},
        "ridges": {"const": {"F64": 3.0}}
    });
    let op: PathOp = serde_json::from_value(json).unwrap();
    match op {
        PathOp::ZigZag { point_type, .. } => assert_eq!(point_type, PointType::Corner),
        other => panic!("expected ZigZag, got {other:?}"),
    }
}

// --- round_corners.radius ≥ 0 ---

#[test]
fn round_corners_negative_radius_rejected() {
    let doc = doc_with_modifiers(vec![PathOp::RoundCorners {
        radius: DocParam::const_f64(-0.1),
    }]);
    assert!(matches!(
        doc.validate(),
        Err(DocumentError::ValueOutOfRange { .. })
    ));
}

// --- trim.start / trim.end ∈ [0, 1] ---

#[test]
fn trim_start_end_out_of_range_rejected() {
    let doc = doc_with_modifiers(vec![PathOp::Trim {
        start: DocParam::const_f64(-0.1),
        end: DocParam::const_f64(1.0),
        offset: DocParam::const_f64(0.0),
        mode: TrimMode::Parallel,
    }]);
    assert!(matches!(
        doc.validate(),
        Err(DocumentError::ValueOutOfRange { .. })
    ));

    let doc = doc_with_modifiers(vec![PathOp::Trim {
        start: DocParam::const_f64(0.0),
        end: DocParam::const_f64(1.1),
        offset: DocParam::const_f64(0.0),
        mode: TrimMode::Sequential,
    }]);
    assert!(matches!(
        doc.validate(),
        Err(DocumentError::ValueOutOfRange { .. })
    ));
}

// --- offset.line_join / miter_limit ---

#[test]
fn offset_defaults_to_miter_join_and_limit_four_on_legacy_json() {
    let json = json!({
        "op": "offset",
        "distance": {"const": {"F64": 0.05}}
    });
    let op: PathOp = serde_json::from_value(json).unwrap();
    match op {
        PathOp::Offset {
            line_join,
            miter_limit,
            ..
        } => {
            assert_eq!(line_join, LineJoin::Miter);
            assert_eq!(miter_limit, 4.0);
        }
        other => panic!("expected Offset, got {other:?}"),
    }
}

#[test]
fn offset_non_positive_miter_limit_rejected() {
    let doc = doc_with_modifiers(vec![PathOp::Offset {
        distance: DocParam::const_f64(0.05),
        line_join: LineJoin::Miter,
        miter_limit: 0.0,
    }]);
    assert!(matches!(
        doc.validate(),
        Err(DocumentError::ValueOutOfRange { .. })
    ));
}

#[test]
fn offset_non_finite_miter_limit_rejected() {
    let doc = doc_with_modifiers(vec![PathOp::Offset {
        distance: DocParam::const_f64(0.05),
        line_join: LineJoin::Round,
        miter_limit: f64::NAN,
    }]);
    assert!(matches!(
        doc.validate(),
        Err(DocumentError::NonFiniteValue { .. })
    ));
}

// --- twist.center必須(旧JSONは型付き拒否。変換はD1e) ---

#[test]
fn twist_valid_center_ok() {
    let doc = doc_with_modifiers(vec![twist(0.5, [0.1, -0.2])]);
    assert!(doc.validate().is_ok());
}

#[test]
fn twist_non_finite_center_rejected() {
    let doc = doc_with_modifiers(vec![twist(0.5, [f64::NAN, 0.0])]);
    assert!(matches!(
        doc.validate(),
        Err(DocumentError::NonFiniteValue { .. })
    ));
}

#[test]
fn twist_legacy_json_without_center_is_rejected() {
    let json = json!({
        "op": "twist",
        "angle": {"const": {"F64": 0.5}}
    });
    let err = serde_json::from_value::<PathOp>(json).unwrap_err();
    assert!(
        err.to_string().contains("center"),
        "expected missing `center` rejection, got {err}"
    );
}

// --- wiggle.seed は u64(非DocParam) ---

#[test]
fn wiggle_seed_is_plain_u64() {
    let json = json!({
        "op": "wiggle",
        "amp": {"const": {"F64": 0.02}},
        "freq": {"const": {"F64": 2.0}},
        "seed": 42
    });
    let op: PathOp = serde_json::from_value(json).unwrap();
    match op {
        PathOp::Wiggle { seed, .. } => assert_eq!(seed, 42u64),
        other => panic!("expected Wiggle, got {other:?}"),
    }
}

#[test]
fn wiggle_legacy_docparam_seed_is_rejected() {
    let json = json!({
        "op": "wiggle",
        "amp": {"const": {"F64": 0.02}},
        "freq": {"const": {"F64": 2.0}},
        "seed": {"const": {"F64": 42.0}}
    });
    assert!(serde_json::from_value::<PathOp>(json).is_err());
}

// --- repeater.transform / composite / opacity ---

#[test]
fn repeater_legacy_json_reads_identity_transform_default() {
    let json = json!({
        "op": "repeater",
        "copies": {"const": {"F64": 3.0}},
        "offset": {"const": {"F64": 0.0}}
    });
    let op: PathOp = serde_json::from_value(json).unwrap();
    match op {
        PathOp::Repeater {
            transform,
            composite,
            start_opacity,
            end_opacity,
            ..
        } => {
            assert_eq!(transform, motolii_doc::Transform2D::identity());
            assert_eq!(composite, CompositeOrder::Above);
            assert_eq!(start_opacity, DocParam::const_f64(1.0));
            assert_eq!(end_opacity, DocParam::const_f64(1.0));
        }
        other => panic!("expected Repeater, got {other:?}"),
    }
}

#[test]
fn repeater_negative_copies_rejected() {
    let doc = doc_with_modifiers(vec![PathOp::Repeater {
        copies: DocParam::const_f64(-1.0),
        offset: DocParam::const_f64(0.0),
        transform: motolii_doc::Transform2D::identity(),
        composite: CompositeOrder::Above,
        start_opacity: DocParam::const_f64(1.0),
        end_opacity: DocParam::const_f64(1.0),
    }]);
    assert!(matches!(
        doc.validate(),
        Err(DocumentError::ValueOutOfRange { .. })
    ));
}

#[test]
fn repeater_fractional_copies_rejected() {
    let doc = doc_with_modifiers(vec![PathOp::Repeater {
        copies: DocParam::const_f64(2.5),
        offset: DocParam::const_f64(0.0),
        transform: motolii_doc::Transform2D::identity(),
        composite: CompositeOrder::Above,
        start_opacity: DocParam::const_f64(1.0),
        end_opacity: DocParam::const_f64(1.0),
    }]);
    assert!(matches!(
        doc.validate(),
        Err(DocumentError::ValueOutOfRange { .. })
    ));
}

#[test]
fn repeater_opacity_out_of_range_rejected() {
    let doc = doc_with_modifiers(vec![PathOp::Repeater {
        copies: DocParam::const_f64(2.0),
        offset: DocParam::const_f64(0.0),
        transform: motolii_doc::Transform2D::identity(),
        composite: CompositeOrder::Below,
        start_opacity: DocParam::const_f64(1.2),
        end_opacity: DocParam::const_f64(1.0),
    }]);
    assert!(matches!(
        doc.validate(),
        Err(DocumentError::ValueOutOfRange { .. })
    ));
}

#[test]
fn repeater_roundtrip_with_full_fields() {
    let op = PathOp::Repeater {
        copies: DocParam::const_f64(4.0),
        offset: DocParam::const_f64(0.5),
        transform: motolii_doc::Transform2D {
            position: DocParam::const_vec2([0.1, 0.0]),
            anchor: DocParam::const_vec2([0.0, 0.0]),
            scale: DocParam::const_vec2([0.9, 0.9]),
            rotation: DocParam::const_f64(0.1),
            z: DocParam::const_f64(0.0),
            parent: None,
        },
        composite: CompositeOrder::Below,
        start_opacity: DocParam::const_f64(1.0),
        end_opacity: DocParam::const_f64(0.2),
    };
    let json = serde_json::to_value(&op).unwrap();
    let back: PathOp = serde_json::from_value(json).unwrap();
    assert_eq!(op, back);
}

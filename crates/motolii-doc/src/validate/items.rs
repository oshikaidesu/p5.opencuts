//! Clip/Group と PathOp/Camera の構造検査。入口の走査から切り離すため。

use std::collections::{HashMap, HashSet};

use motolii_core::RationalTime;

use crate::param_expect::{self, path_op_scalar, ParamConstraints};
use crate::schema::{
    Clip, ClipSource, CompCameraDoc, Group, ItemEnvelope, StreamKind, TrackItem, Transform2D,
};
use crate::track_id::TrackId;
use crate::{Document, LayerId};

use super::params::{validate_param, validate_param_structure, validate_vector_content};
use super::DocumentError;

impl Document {
    pub(super) fn require_track(&self, id: TrackId) -> Result<(), DocumentError> {
        if self.track_ids.contains(id) {
            Ok(())
        } else {
            Err(DocumentError::UnknownTrackId { id: id.get() })
        }
    }

    pub(super) fn require_layer(&self, id: LayerId) -> Result<(), DocumentError> {
        if self.layers.contains(id) {
            Ok(())
        } else {
            Err(DocumentError::UnknownLayerId { id: id.get() })
        }
    }
}

pub(super) fn validate_item(
    doc: &Document,
    item: &TrackItem,
    seen_layers: &mut HashSet<u64>,
    parents: &mut HashMap<u64, u64>,
) -> Result<(), DocumentError> {
    match item {
        TrackItem::Clip(clip) => validate_clip(doc, clip, seen_layers, parents),
        TrackItem::Group(group) => validate_group(doc, group, seen_layers, parents),
    }
}

fn validate_group(
    doc: &Document,
    group: &Group,
    seen_layers: &mut HashSet<u64>,
    parents: &mut HashMap<u64, u64>,
) -> Result<(), DocumentError> {
    validate_envelope(doc, &group.envelope, seen_layers, parents)?;
    for child in &group.children {
        validate_item(doc, child, seen_layers, parents)?;
    }
    Ok(())
}

fn validate_clip(
    doc: &Document,
    clip: &Clip,
    seen_layers: &mut HashSet<u64>,
    parents: &mut HashMap<u64, u64>,
) -> Result<(), DocumentError> {
    let layer_id = clip.envelope.layer_id.get();
    validate_envelope(doc, &clip.envelope, seen_layers, parents)?;

    if clip.duration <= RationalTime::ZERO {
        return Err(DocumentError::NonPositiveClipDuration { layer_id });
    }
    // start の下限は検査しない: 負開始を許容(トリムイン相当。AM/AE互換)。
    // 区間正当性は duration>0 と半開終端 end <= composition.duration のみ。
    let end = clip
        .start
        .try_add(clip.duration)
        .map_err(|_| DocumentError::ClipIntervalOverflow { layer_id })?;
    if end > doc.composition.duration {
        return Err(DocumentError::ClipPastComposition {
            layer_id,
            end,
            comp: doc.composition.duration,
        });
    }

    // TimeMapはフィールドがpubのためedit経路で壊せる — deserialize拒否と同じ不変条件を保存前にも強制(監査T-2)
    clip.time_map
        .validate()
        .map_err(|source| DocumentError::InvalidTimeMap { layer_id, source })?;

    match &clip.source {
        ClipSource::Asset {
            asset: _,
            video,
            audio,
        } => {
            if video.is_none() && audio.is_empty() {
                return Err(DocumentError::EmptyAssetComponents { layer_id });
            }
            if let Some(video) = video {
                if video.stream.kind != StreamKind::Video {
                    return Err(DocumentError::VideoComponentKindMismatch { layer_id });
                }
                if video.stream.ordinal != 0 {
                    return Err(DocumentError::UnsupportedVideoStreamOrdinal {
                        layer_id,
                        ordinal: video.stream.ordinal,
                    });
                }
            }
            for (index, comp) in audio.iter().enumerate() {
                if comp.stream.kind != StreamKind::Audio {
                    return Err(DocumentError::AudioComponentKindMismatch { layer_id, index });
                }
                if let Some(first_index) = audio[..index]
                    .iter()
                    .position(|earlier| earlier.stream.ordinal == comp.stream.ordinal)
                {
                    return Err(DocumentError::DuplicateAudioStreamOrdinal {
                        layer_id,
                        ordinal: comp.stream.ordinal,
                        first_index,
                        second_index: index,
                    });
                }
                validate_param(
                    doc,
                    &comp.gain,
                    ParamConstraints::min_f64(0.0),
                    &format!("layer{layer_id}.source.audio[{index}].gain"),
                )?;
            }
        }
        ClipSource::Plugin {
            plugin_id, params, ..
        } => {
            if plugin_id.is_empty() {
                return Err(DocumentError::EmptySourcePluginId { layer_id });
            }
            let source_path = format!("layer{layer_id}.source");
            for (name, param) in params {
                let path = format!("{source_path}.{name}");
                validate_param_structure(doc, param, &path)?;
            }
        }
        ClipSource::Vector { recipe } => {
            validate_vector_content(doc, &recipe.content, &format!("layer{layer_id}.recipe"))?;
            for (i, op) in recipe.modifiers.iter().enumerate() {
                validate_path_op_params(
                    doc,
                    op,
                    &format!("layer{layer_id}.recipe.modifiers[{i}]"),
                )?;
            }
        }
    }

    Ok(())
}

fn validate_envelope(
    doc: &Document,
    env: &ItemEnvelope,
    seen_layers: &mut HashSet<u64>,
    parents: &mut HashMap<u64, u64>,
) -> Result<(), DocumentError> {
    let id = env.layer_id.get();
    doc.require_layer(env.layer_id)?;
    if !seen_layers.insert(id) {
        return Err(DocumentError::DuplicateLayerId { id });
    }
    if let Some(parent) = env.transform.parent {
        doc.require_layer(parent)?;
        parents.insert(id, parent.get());
    }
    let base = format!("layer{id}");
    validate_transform2d(doc, &env.transform, &base)?;
    validate_param(
        doc,
        &env.opacity,
        param_expect::envelope_opacity(),
        &format!("{base}.opacity"),
    )?;
    for use_ in &env.effects {
        if doc.effect_definition(use_.definition_id).is_none() {
            return Err(DocumentError::DanglingEffectDefinition {
                layer_id: id,
                id: use_.id.get(),
                definition_id: use_.definition_id.get(),
            });
        }
    }
    Ok(())
}

/// D1l: 文書内のいずれかのlayer(Group含む)が1つでも`EffectUse`を持つか。
pub(super) fn document_has_any_effect_use(doc: &Document) -> bool {
    fn walk(items: &[TrackItem]) -> bool {
        items.iter().any(|item| {
            let (effects, children): (&[crate::schema::EffectUse], &[TrackItem]) = match item {
                TrackItem::Clip(clip) => (&clip.envelope.effects, &[]),
                TrackItem::Group(group) => (&group.envelope.effects, &group.children),
            };
            !effects.is_empty() || walk(children)
        })
    }
    doc.tracks.iter().any(|t| walk(&t.items))
}

/// Transform2Dのスロット共通検査。エンベロープ本体とRepeater.transformで共用(D1i-2)。
fn validate_transform2d(doc: &Document, t: &Transform2D, base: &str) -> Result<(), DocumentError> {
    validate_param(
        doc,
        &t.position,
        param_expect::transform_position(),
        &format!("{base}.position"),
    )?;
    validate_param(
        doc,
        &t.anchor,
        param_expect::transform_anchor(),
        &format!("{base}.anchor"),
    )?;
    validate_param(
        doc,
        &t.scale,
        param_expect::transform_scale(),
        &format!("{base}.scale"),
    )?;
    validate_param(
        doc,
        &t.rotation,
        param_expect::transform_rotation(),
        &format!("{base}.rotation"),
    )?;
    // 奥行きはスカラー1本。LookAt/Followは位置と角度の語彙なのでここには来ない。
    validate_param(
        doc,
        &t.z,
        ParamConstraints::scalar_f64(),
        &format!("{base}.z"),
    )
}

pub(super) fn detect_parent_cycles(parents: &HashMap<u64, u64>) -> Result<(), DocumentError> {
    for &start in parents.keys() {
        let mut path = HashSet::new();
        let mut cur = start;
        loop {
            if !path.insert(cur) {
                return Err(DocumentError::ParentCycle { layer_id: cur });
            }
            match parents.get(&cur) {
                Some(&p) if p == cur => {
                    return Err(DocumentError::ParentCycle { layer_id: cur });
                }
                Some(&p) => cur = p,
                None => break,
            }
        }
    }
    Ok(())
}

pub(super) fn validate_comp_camera_doc(
    doc: &Document,
    camera: &CompCameraDoc,
    path: &str,
) -> Result<(), DocumentError> {
    match camera {
        CompCameraDoc::PlanarOrthographic {
            center,
            roll_radians,
            height,
        } => {
            validate_param(
                doc,
                center,
                param_expect::planar_camera_center(),
                &format!("{path}.center"),
            )?;
            validate_param(
                doc,
                roll_radians,
                param_expect::planar_camera_roll(),
                &format!("{path}.roll_radians"),
            )?;
            validate_param(
                doc,
                height,
                param_expect::planar_camera_height(),
                &format!("{path}.height"),
            )
        }
    }
}

/// PathOp意味論表(D1i-2)の拒否項目をここで型付きエラーに落とす。
/// open-path Offsetの拒否は幾何側(`pathgeom::apply`)の責務 — validateはDocumentの
/// 静的スキーマしか見えず、SvgAsset/TextPath由来パスの開閉はレシピからは判定できない。
fn validate_path_op_params(
    doc: &Document,
    op: &crate::schema::PathOp,
    path: &str,
) -> Result<(), DocumentError> {
    use crate::schema::PathOp;
    let scalar = path_op_scalar();
    match op {
        PathOp::PuckerBloat { amount } => validate_param(
            doc,
            amount,
            param_expect::path_op_pucker_bloat_amount(),
            &format!("{path}.amount"),
        ),
        PathOp::ZigZag {
            amount,
            ridges,
            point_type: _,
        } => {
            validate_param(
                doc,
                amount,
                param_expect::path_op_non_negative(),
                &format!("{path}.amount"),
            )?;
            validate_param(
                doc,
                ridges,
                param_expect::path_op_non_negative(),
                &format!("{path}.ridges"),
            )
        }
        PathOp::Offset {
            distance,
            line_join: _,
            miter_limit,
        } => {
            validate_param(doc, distance, scalar, &format!("{path}.distance"))?;
            if !miter_limit.is_finite() {
                return Err(DocumentError::NonFiniteValue {
                    path: format!("{path}.miter_limit"),
                });
            }
            if *miter_limit <= 0.0 {
                return Err(DocumentError::ValueOutOfRange {
                    path: format!("{path}.miter_limit"),
                });
            }
            Ok(())
        }
        PathOp::RoundCorners { radius } => validate_param(
            doc,
            radius,
            param_expect::path_op_non_negative(),
            &format!("{path}.radius"),
        ),
        PathOp::Trim {
            start,
            end,
            offset,
            mode: _,
        } => {
            validate_param(
                doc,
                start,
                param_expect::path_op_unit_interval(),
                &format!("{path}.start"),
            )?;
            validate_param(
                doc,
                end,
                param_expect::path_op_unit_interval(),
                &format!("{path}.end"),
            )?;
            validate_param(doc, offset, scalar, &format!("{path}.offset"))
        }
        PathOp::Twist { angle, center } => {
            validate_param(doc, angle, scalar, &format!("{path}.angle"))?;
            validate_param(
                doc,
                center,
                param_expect::path_op_vec2(),
                &format!("{path}.center"),
            )
        }
        PathOp::Wiggle { amp, freq, seed: _ } => {
            validate_param(doc, amp, scalar, &format!("{path}.amp"))?;
            validate_param(doc, freq, scalar, &format!("{path}.freq"))
            // seedはu64固定(非DocParam) — 型で非有限値・キーフレームを構文上排除済み。
        }
        PathOp::Repeater {
            copies,
            offset,
            transform,
            composite: _,
            start_opacity,
            end_opacity,
        } => {
            validate_param(
                doc,
                copies,
                param_expect::path_op_non_negative_integer(),
                &format!("{path}.copies"),
            )?;
            validate_param(doc, offset, scalar, &format!("{path}.offset"))?;
            validate_transform2d(doc, transform, &format!("{path}.transform"))?;
            validate_param(
                doc,
                start_opacity,
                param_expect::path_op_opacity(),
                &format!("{path}.start_opacity"),
            )?;
            validate_param(
                doc,
                end_opacity,
                param_expect::path_op_opacity(),
                &format!("{path}.end_opacity"),
            )
        }
    }
}

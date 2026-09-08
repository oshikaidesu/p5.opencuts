//! AssetRef 結線の棚卸し。検証入口と stable-id 走査から切り離すため。

use crate::asset::AssetId;
use crate::doc_value::DocValue;
use crate::param::DocParam;
use crate::schema::{
    Clip, ClipSource, CompCameraDoc, Group, ItemEnvelope, PathOp, StandardShape, TrackItem,
    Transform2D, VectorContent,
};

#[derive(Debug, Clone)]
pub(crate) struct AssetUse {
    pub(crate) id: AssetId,
    pub(super) path: String,
    pub(super) allowed_types: &'static [&'static str],
}

/// `VectorContent::SvgAsset` が要求する MIME。
const SVG_ASSET_TYPE: &str = "image/svg+xml";
const SVG_ASSET_TYPES: &[&str] = &[SVG_ASSET_TYPE];

/// `TextPath.font_asset` の許可型(D1i-1で確定。未決を埋めずここで正本化)。
const FONT_ASSET_TYPES: &[&str] = &["font/ttf", "font/otf", "font/woff", "font/woff2"];

pub(super) fn asset_use(
    id: AssetId,
    path: impl Into<String>,
    allowed_types: &'static [&'static str],
) -> AssetUse {
    AssetUse {
        id,
        path: path.into(),
        allowed_types,
    }
}

pub(super) fn collect_asset_uses_item(item: &TrackItem, out: &mut Vec<AssetUse>) {
    let (envelope, source, children) = match item {
        TrackItem::Clip(clip) => {
            let Clip {
                envelope,
                start: _,
                duration: _,
                time_map: _,
                source,
            } = clip;
            (envelope, Some(source), &[][..])
        }
        TrackItem::Group(group) => {
            let Group { envelope, children } = group;
            (envelope, None, children.as_slice())
        }
    };
    let base = format!("layer{}", envelope.layer_id.get());
    let ItemEnvelope {
        layer_id: _,
        effects: _,
        // 色は asset を参照しない
        color: _,
        transform,
        clipping_mask: _,
        blend: _,
        opacity,
        visible: _,
        solo: _,
        lock: _,
    } = envelope;
    collect_asset_uses_transform(transform, &format!("{base}.transform"), out);
    collect_asset_uses_param(opacity, &format!("{base}.opacity"), out);

    if let Some(source) = source {
        match source {
            ClipSource::Asset {
                asset,
                video: _,
                audio,
            } => {
                out.push(asset_use(*asset, format!("{base}.source.asset"), &[]));
                for (index, component) in audio.iter().enumerate() {
                    let crate::schema::AudioComponent {
                        stream: _,
                        enabled: _,
                        gain,
                        out_of_range: _,
                    } = component;
                    collect_asset_uses_param(
                        gain,
                        &format!("{base}.source.audio[{index}].gain"),
                        out,
                    );
                }
            }
            ClipSource::Plugin {
                plugin_id: _,
                effect_version: _,
                params,
                extra: _,
            } => {
                for (name, param) in params {
                    collect_asset_uses_param(param, &format!("{base}.source.{name}"), out);
                }
            }
            ClipSource::Vector { recipe } => {
                let crate::schema::VectorRecipe { content, modifiers } = recipe;
                collect_asset_uses_vector_content(content, &format!("{base}.recipe"), out);
                for (index, op) in modifiers.iter().enumerate() {
                    collect_asset_uses_path_op(
                        op,
                        &format!("{base}.recipe.modifiers[{index}]"),
                        out,
                    );
                }
            }
        }
    }
    for child in children {
        collect_asset_uses_item(child, out);
    }
}

pub(super) fn collect_asset_uses_comp_camera(camera: &CompCameraDoc, out: &mut Vec<AssetUse>) {
    match camera {
        CompCameraDoc::PlanarOrthographic {
            center,
            roll_radians,
            height,
        } => {
            collect_asset_uses_param(center, "composition.camera.center", out);
            collect_asset_uses_param(roll_radians, "composition.camera.roll_radians", out);
            collect_asset_uses_param(height, "composition.camera.height", out);
        }
    }
}

fn collect_asset_uses_transform(transform: &Transform2D, base: &str, out: &mut Vec<AssetUse>) {
    let Transform2D {
        position,
        anchor,
        scale,
        rotation,
        z,
        parent: _,
    } = transform;
    collect_asset_uses_param(position, &format!("{base}.position"), out);
    collect_asset_uses_param(anchor, &format!("{base}.anchor"), out);
    collect_asset_uses_param(scale, &format!("{base}.scale"), out);
    collect_asset_uses_param(rotation, &format!("{base}.rotation"), out);
    collect_asset_uses_param(z, &format!("{base}.z"), out);
}

pub(super) fn collect_asset_uses_param(param: &DocParam, path: &str, out: &mut Vec<AssetUse>) {
    match param {
        DocParam::Const(value) => collect_asset_uses_value(value, path, out),
        DocParam::Keyframes(track) => {
            for key in track.keys() {
                collect_asset_uses_value(&key.value, path, out);
            }
        }
        DocParam::Data { track: _, fallback } => collect_asset_uses_value(fallback, path, out),
        DocParam::Vec2Axes { x, y } => {
            collect_asset_uses_param(x, &format!("{path}.x"), out);
            collect_asset_uses_param(y, &format!("{path}.y"), out);
        }
        DocParam::LookAt { target: _, axis: _ }
        | DocParam::Follow {
            target: _,
            offset: _,
        } => {}
    }
}

fn collect_asset_uses_value(value: &DocValue, path: &str, out: &mut Vec<AssetUse>) {
    if let DocValue::AssetRef(id) = value {
        out.push(asset_use(*id, path, &[]));
    }
}

fn collect_asset_uses_vector_content(content: &VectorContent, path: &str, out: &mut Vec<AssetUse>) {
    match content {
        VectorContent::StandardShape { shape } => match shape {
            StandardShape::Rect { width, height } | StandardShape::Ellipse { width, height } => {
                collect_asset_uses_param(width, &format!("{path}.width"), out);
                collect_asset_uses_param(height, &format!("{path}.height"), out);
            }
        },
        VectorContent::SvgAsset { asset } => {
            out.push(asset_use(*asset, format!("{path}.asset"), SVG_ASSET_TYPES))
        }
        VectorContent::TextPath {
            text: _,
            font_asset,
        } => out.push(asset_use(
            *font_asset,
            format!("{path}.font_asset"),
            FONT_ASSET_TYPES,
        )),
        VectorContent::Group { children } => {
            for (index, child) in children.iter().enumerate() {
                collect_asset_uses_vector_content(child, &format!("{path}.children[{index}]"), out);
            }
        }
    }
}

fn collect_asset_uses_path_op(op: &PathOp, path: &str, out: &mut Vec<AssetUse>) {
    match op {
        PathOp::PuckerBloat { amount } => {
            collect_asset_uses_param(amount, &format!("{path}.amount"), out)
        }
        PathOp::ZigZag {
            amount,
            ridges,
            point_type: _,
        } => {
            collect_asset_uses_param(amount, &format!("{path}.amount"), out);
            collect_asset_uses_param(ridges, &format!("{path}.ridges"), out);
        }
        PathOp::Offset {
            distance,
            line_join: _,
            miter_limit: _,
        } => collect_asset_uses_param(distance, &format!("{path}.distance"), out),
        PathOp::RoundCorners { radius } => {
            collect_asset_uses_param(radius, &format!("{path}.radius"), out)
        }
        PathOp::Trim {
            start,
            end,
            offset,
            mode: _,
        } => {
            collect_asset_uses_param(start, &format!("{path}.start"), out);
            collect_asset_uses_param(end, &format!("{path}.end"), out);
            collect_asset_uses_param(offset, &format!("{path}.offset"), out);
        }
        PathOp::Twist { angle, center } => {
            collect_asset_uses_param(angle, &format!("{path}.angle"), out);
            collect_asset_uses_param(center, &format!("{path}.center"), out);
        }
        PathOp::Wiggle { amp, freq, seed: _ } => {
            collect_asset_uses_param(amp, &format!("{path}.amp"), out);
            collect_asset_uses_param(freq, &format!("{path}.freq"), out);
        }
        PathOp::Repeater {
            copies,
            offset,
            transform,
            composite: _,
            start_opacity,
            end_opacity,
        } => {
            collect_asset_uses_param(copies, &format!("{path}.copies"), out);
            collect_asset_uses_param(offset, &format!("{path}.offset"), out);
            collect_asset_uses_transform(transform, &format!("{path}.transform"), out);
            collect_asset_uses_param(start_opacity, &format!("{path}.start_opacity"), out);
            collect_asset_uses_param(end_opacity, &format!("{path}.end_opacity"), out);
        }
    }
}

#[cfg(test)]
mod asset_use_tests {
    use std::collections::{BTreeMap, BTreeSet};

    use motolii_core::RationalTime;

    use super::*;
    use crate::schema::{EffectDefinition, Track};
    use crate::track_id::TrackId;
    use crate::validate::DocumentError;
    use crate::EffectDefinitionId;
    use crate::{Document, LayerId};

    fn asset_ref(id: AssetId) -> DocParam {
        DocParam::Const(DocValue::AssetRef(id))
    }

    fn clip(layer: u64, source: ClipSource) -> TrackItem {
        TrackItem::Clip(Clip {
            envelope: ItemEnvelope::new(LayerId::from_raw(layer)),
            start: RationalTime::ZERO,
            duration: RationalTime::try_new(1, 1).unwrap(),
            time_map: Default::default(),
            source,
        })
    }

    #[test]
    fn asset_use_inventory_covers_camera_orphan_and_recursive_group() {
        let asset = AssetId::from_raw(10);
        let mut doc = Document::new_current();
        doc.composition.camera = CompCameraDoc::PlanarOrthographic {
            center: asset_ref(asset),
            roll_radians: DocParam::const_f64(0.0),
            height: DocParam::const_f64(1.0),
        };

        let mut group_envelope = ItemEnvelope::new(LayerId::from_raw(1));
        group_envelope.opacity = asset_ref(asset);
        doc.tracks.push(Track {
            id: TrackId::from_raw(0),
            items: vec![TrackItem::Group(Group {
                envelope: group_envelope,
                children: vec![clip(
                    2,
                    ClipSource::Plugin {
                        plugin_id: "test.source".into(),
                        effect_version: 1,
                        params: BTreeMap::from([("texture".into(), asset_ref(asset))]),
                        extra: Default::default(),
                    },
                )],
            })],
        });
        doc.effect_definitions.push(EffectDefinition::new(
            EffectDefinitionId::from_raw(0),
            "test.effect",
            1,
            true,
            BTreeMap::from([("texture".into(), asset_ref(asset))]),
            Default::default(),
        ));

        let uses = doc.asset_uses();
        let paths = uses
            .iter()
            .map(|asset_use| asset_use.path.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            paths,
            BTreeSet::from([
                "composition.camera.center",
                "effect_definitions[0].texture",
                "layer1.opacity",
                "layer2.source.texture",
            ])
        );
    }

    #[test]
    fn asset_use_inventory_rejects_dangling_orphan_effect_reference() {
        let mut doc = Document::new_current();
        let definition_id = EffectDefinitionId::from_raw(doc.next_stable_id.allocate().unwrap());
        doc.effect_definitions.push(EffectDefinition::new(
            definition_id,
            "test.effect",
            1,
            true,
            BTreeMap::from([("texture".into(), asset_ref(AssetId::from_raw(99)))]),
            Default::default(),
        ));

        assert_eq!(
            doc.validate(),
            Err(DocumentError::UnknownAssetId { id: 99 })
        );
    }
}

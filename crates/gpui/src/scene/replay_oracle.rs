//! A draw-order oracle for scene replay.
//!
//! Two finished scenes agree when every pair of primitives whose clipped bounds
//! overlap is drawn in the same order by both, and both draw the same primitives.
//! Pairs that do not overlap may be drawn in any order: batching reorders them.
//! "Overlap" is [`Bounds::intersects`], the rule the bounds tree orders by.
//!
//! Fixtures are scripts of primitives and layers. Every primitive carries an id
//! that survives replay and sorting (a sprite's tile id, every other kind's
//! color), so a sequence of draws can be compared across scenes.

use super::*;
use crate::{
    AtlasTextureId, AtlasTextureKind, Hsla, TileId, TimedTransformation, Transformation, point, px,
    size,
};
use collections::FxHashMap;
use rand::{Rng as _, rngs::StdRng};
use std::time::Duration;

/// Ids are encoded in a color's hue as `id / ID_SCALE`, exact in an `f32` below 2^24.
const ID_SCALE: f32 = (1 << 20) as f32;
const EXTENT: f32 = 512.;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Kind {
    Shadow,
    Quad,
    Underline,
    MonochromeSprite,
    SubpixelSprite,
    PolychromeSprite,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Op {
    Primitive {
        kind: Kind,
        id: u32,
        bounds: Bounds<ScaledPixels>,
        content_mask: Bounds<ScaledPixels>,
        texture: u32,
    },
    AnimatedSprite {
        id: u32,
        bounds: Bounds<ScaledPixels>,
        content_mask: Bounds<ScaledPixels>,
        texture: u32,
    },
    StartLayer,
    EndLayer,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Drawn {
    pub kind: Kind,
    pub id: u32,
    pub bounds: Bounds<ScaledPixels>,
}

fn id_color(id: u32) -> Hsla {
    Hsla {
        h: id as f32 / ID_SCALE,
        s: 0.5,
        l: 0.5,
        a: 1.,
    }
}

fn color_id(color: Hsla) -> u32 {
    (color.h * ID_SCALE).round() as u32
}

fn primitive(
    kind: Kind,
    id: u32,
    bounds: Bounds<ScaledPixels>,
    mask: Bounds<ScaledPixels>,
    texture: u32,
) -> Primitive {
    let content_mask = ContentMask { bounds: mask };
    let tile = AtlasTile {
        texture_id: AtlasTextureId {
            index: texture,
            kind: match kind {
                Kind::PolychromeSprite => AtlasTextureKind::Polychrome,
                _ => AtlasTextureKind::Monochrome,
            },
        },
        tile_id: TileId(id),
        padding: 0,
        bounds: Bounds::default(),
    };
    match kind {
        Kind::Shadow => Shadow {
            order: 0,
            blur_radius: ScaledPixels(4.),
            bounds,
            corner_radii: Corners::default(),
            content_mask,
            color: id_color(id),
            shape_bounds: bounds,
            shape_corner_radii: Corners::default(),
            inset: false.into(),
            pad: 0,
        }
        .into(),
        Kind::Quad => Quad {
            bounds,
            content_mask,
            background: id_color(id).into(),
            ..Default::default()
        }
        .into(),
        Kind::Underline => Underline {
            order: 0,
            pad: 0,
            bounds,
            content_mask,
            color: id_color(id),
            thickness: ScaledPixels(1.),
            wavy: Default::default(),
        }
        .into(),
        Kind::MonochromeSprite => MonochromeSprite {
            order: 0,
            animation: SpriteAnimationId::NONE,
            tile,
            bounds,
            content_mask,
            color: id_color(0),
            transformation: TransformationMatrix::unit(),
        }
        .into(),
        Kind::SubpixelSprite => SubpixelSprite {
            order: 0,
            pad: 0,
            tile,
            bounds,
            content_mask,
            color: id_color(0),
            transformation: TransformationMatrix::unit(),
        }
        .into(),
        Kind::PolychromeSprite => PolychromeSprite {
            order: 0,
            pad: 0,
            tile,
            grayscale: false.into(),
            opacity: 1.,
            bounds,
            content_mask,
            corner_radii: Corners::default(),
        }
        .into(),
    }
}

/// Paints `script` into `scene` as an element tree would.
pub(super) fn paint(scene: &mut Scene, script: &[Op]) {
    for op in script {
        match *op {
            Op::Primitive {
                kind,
                id,
                bounds,
                content_mask,
                texture,
            } => scene.insert_primitive(primitive(kind, id, bounds, content_mask, texture)),
            Op::AnimatedSprite {
                id,
                bounds,
                content_mask,
                texture,
            } => {
                let sprite = MonochromeSprite {
                    order: 0,
                    animation: SpriteAnimationId::NONE,
                    tile: AtlasTile {
                        texture_id: AtlasTextureId {
                            index: texture,
                            kind: AtlasTextureKind::Monochrome,
                        },
                        tile_id: TileId(id),
                        padding: 0,
                        bounds: Bounds::default(),
                    },
                    bounds,
                    content_mask: ContentMask {
                        bounds: content_mask,
                    },
                    color: id_color(0),
                    transformation: TransformationMatrix::unit(),
                };
                scene.insert_animated_sprite(
                    sprite,
                    SpriteAnimation::new(
                        TimedTransformation::rotation(
                            Duration::from_secs(1),
                            Transformation::default(),
                        ),
                        point(px(8.), px(8.)),
                        1.,
                    ),
                );
            }
            Op::StartLayer => scene.push_layer(),
            Op::EndLayer => scene.pop_layer(),
        }
    }
}

/// The primitives of a finished scene in the order its batches draw them.
pub(super) fn draw_sequence(scene: &Scene) -> Vec<Drawn> {
    let mut sequence = Vec::new();
    let mut push = |kind, id, primitive: Primitive| {
        sequence.push(Drawn {
            kind,
            id,
            bounds: primitive.clipped_bounds(),
        })
    };
    for batch in scene.batches() {
        match batch {
            PrimitiveBatch::Shadows(range) => {
                for shadow in &scene.shadows[range] {
                    push(Kind::Shadow, color_id(shadow.color), (*shadow).into());
                }
            }
            PrimitiveBatch::Quads(range) => {
                for quad in &scene.quads[range] {
                    push(Kind::Quad, color_id(quad.background.solid), (*quad).into());
                }
            }
            PrimitiveBatch::Underlines(range) => {
                for underline in &scene.underlines[range] {
                    push(
                        Kind::Underline,
                        color_id(underline.color),
                        (*underline).into(),
                    );
                }
            }
            PrimitiveBatch::MonochromeSprites { range, .. } => {
                for sprite in &scene.monochrome_sprites[range] {
                    push(
                        Kind::MonochromeSprite,
                        sprite.tile.tile_id.0,
                        (*sprite).into(),
                    );
                }
            }
            PrimitiveBatch::PolychromeSprites { range, .. } => {
                for sprite in &scene.polychrome_sprites[range] {
                    push(
                        Kind::PolychromeSprite,
                        sprite.tile.tile_id.0,
                        (*sprite).into(),
                    );
                }
            }
            PrimitiveBatch::SubpixelSprites { range, .. } => {
                for sprite in &scene.subpixel_sprites[range] {
                    push(
                        Kind::SubpixelSprite,
                        sprite.tile.tile_id.0,
                        (*sprite).into(),
                    );
                }
            }
            PrimitiveBatch::Paths(_) | PrimitiveBatch::Surfaces(_) => {
                panic!("oracle fixtures paint no paths, subpixel sprites or surfaces")
            }
        }
    }
    sequence
}

/// Why `actual` does not draw like `expected`, or `None` when it does.
pub(super) fn draw_order_mismatch(expected: &Scene, actual: &Scene) -> Option<String> {
    let expected = draw_sequence(expected);
    let actual = draw_sequence(actual);
    if expected.len() != actual.len() {
        return Some(format!(
            "expected {} primitives, drew {}",
            expected.len(),
            actual.len()
        ));
    }
    let position = actual
        .iter()
        .enumerate()
        .map(|(index, drawn)| ((drawn.kind, drawn.id), index))
        .collect::<FxHashMap<_, _>>();
    let mut positions = Vec::with_capacity(expected.len());
    for drawn in &expected {
        match position.get(&(drawn.kind, drawn.id)) {
            Some(index) => positions.push(*index),
            None => return Some(format!("{:?} {} was not drawn", drawn.kind, drawn.id)),
        }
    }
    for (first, below) in expected.iter().enumerate() {
        for (second, above) in expected.iter().enumerate().skip(first + 1) {
            if below.bounds.intersects(&above.bounds) && positions[first] > positions[second] {
                return Some(format!(
                    "{:?} {} is drawn over {:?} {}, which should cover it",
                    below.kind, below.id, above.kind, above.id
                ));
            }
        }
    }
    None
}

#[derive(Clone, Copy)]
pub(super) struct ScriptShape {
    pub primitives: usize,
    pub max_layer_depth: usize,
    /// Chance in 0..=1 that a primitive's content mask clips it, possibly away.
    pub clip_chance: f64,
}

/// A random script with balanced layers, each primitive a fresh id from `next_id`.
pub(super) fn random_script(rng: &mut StdRng, shape: ScriptShape, next_id: &mut u32) -> Vec<Op> {
    let kinds = [
        Kind::Shadow,
        Kind::Quad,
        Kind::Underline,
        Kind::MonochromeSprite,
        Kind::PolychromeSprite,
    ];
    let mut script = Vec::new();
    let mut depth = 0;
    for _ in 0..shape.primitives {
        if depth < shape.max_layer_depth && rng.random_bool(0.15) {
            script.push(Op::StartLayer);
            depth += 1;
        } else if depth > 0 && rng.random_bool(0.15) {
            script.push(Op::EndLayer);
            depth -= 1;
        }
        let origin = point(
            ScaledPixels(rng.random_range(0. ..EXTENT)),
            ScaledPixels(rng.random_range(0. ..EXTENT)),
        );
        let extent = if rng.random_bool(0.8) { 24. } else { 160. };
        let bounds = Bounds {
            origin,
            size: size(
                ScaledPixels(rng.random_range(2. ..extent)),
                ScaledPixels(rng.random_range(2. ..extent)),
            ),
        };
        let content_mask = if rng.random_bool(shape.clip_chance) {
            Bounds {
                origin: point(
                    ScaledPixels(rng.random_range(0. ..EXTENT)),
                    ScaledPixels(rng.random_range(0. ..EXTENT)),
                ),
                size: size(
                    ScaledPixels(rng.random_range(0. ..EXTENT / 2.)),
                    ScaledPixels(rng.random_range(0. ..EXTENT / 2.)),
                ),
            }
        } else {
            Bounds {
                origin: Point::default(),
                size: size(ScaledPixels(EXTENT * 2.), ScaledPixels(EXTENT * 2.)),
            }
        };
        script.push(Op::Primitive {
            kind: kinds[rng.random_range(0..kinds.len())],
            id: *next_id,
            bounds,
            content_mask,
            texture: rng.random_range(0..3),
        });
        *next_id += 1;
    }
    script.extend(std::iter::repeat_n(Op::EndLayer, depth));
    script
}

/// A cached view's range: `range` painted between a `before` and an `after`,
/// recorded in one frame and replayed into the next between other content.
pub(super) struct ReplayCase {
    pub recorded: Scene,
    pub range: Range<usize>,
    pub before: Vec<Op>,
    pub view: Vec<Op>,
    pub after: Vec<Op>,
}

impl ReplayCase {
    pub(super) fn random(rng: &mut StdRng, shape: ScriptShape) -> Self {
        let mut next_id = 1;
        let surround = ScriptShape {
            primitives: shape.primitives / 2,
            ..shape
        };
        let recorded_before = random_script(rng, surround, &mut next_id);
        let view = random_script(rng, shape, &mut next_id);
        let recorded_after = random_script(rng, surround, &mut next_id);
        let mut recorded = Scene::default();
        paint(&mut recorded, &recorded_before);
        let start = recorded.len();
        paint(&mut recorded, &view);
        let range = start..recorded.len();
        paint(&mut recorded, &recorded_after);
        recorded.finish();
        Self {
            recorded,
            range,
            before: random_script(rng, surround, &mut next_id),
            view,
            after: random_script(rng, surround, &mut next_id),
        }
    }

    /// The frame as a full render paints it.
    pub(super) fn rendered(&self) -> Scene {
        let mut scene = Scene::default();
        paint(&mut scene, &self.before);
        paint(&mut scene, &self.view);
        paint(&mut scene, &self.after);
        scene.finish();
        scene
    }

    /// The frame with the view's range replayed by `replay`.
    pub(super) fn replayed(&self, replay: impl FnOnce(&mut Scene, Range<usize>, &Scene)) -> Scene {
        let mut scene = Scene::default();
        paint(&mut scene, &self.before);
        replay(&mut scene, self.range.clone(), &self.recorded);
        paint(&mut scene, &self.after);
        scene.finish();
        scene
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng as _;

    type Replay = fn(&mut Scene, Range<usize>, &Scene);

    fn per_operation_replay(scene: &mut Scene, range: Range<usize>, recorded: &Scene) {
        scene.replay(range, recorded);
    }

    /// Replays as `Window::reuse_paint` must: passing the depth of the layers
    /// open where the range was recorded.
    fn one_insert_replay(scene: &mut Scene, range: Range<usize>, recorded: &Scene) {
        let mut depth = 0usize;
        for operation in &recorded.paint_operations[..range.start] {
            match operation {
                PaintOperation::StartLayer => depth += 1,
                PaintOperation::EndLayer => depth -= 1,
                PaintOperation::Primitive(_) => {}
            }
        }
        scene.replay_with_one_insert(range, recorded, depth);
    }

    const REPLAYS: [(&str, Replay); 2] = [
        ("per-operation", per_operation_replay),
        ("one-insert", one_insert_replay),
    ];

    fn square(id: u32, x: f32) -> Op {
        let bounds = Bounds {
            origin: point(ScaledPixels(x), ScaledPixels(0.)),
            size: size(ScaledPixels(10.), ScaledPixels(10.)),
        };
        Op::Primitive {
            kind: Kind::Quad,
            id,
            bounds,
            content_mask: bounds,
            texture: 0,
        }
    }

    fn finished(script: &[Op]) -> Scene {
        let mut scene = Scene::default();
        paint(&mut scene, script);
        scene.finish();
        scene
    }

    fn assert_case(case: &ReplayCase, replay: Replay, description: impl std::fmt::Display) {
        let replayed = case.replayed(replay);
        if let Some(mismatch) = draw_order_mismatch(&case.rendered(), &replayed) {
            panic!("{description}: {mismatch}");
        }
    }

    fn random_case(seed: u64, shape: ScriptShape) -> ReplayCase {
        let mut random = StdRng::seed_from_u64(seed);
        ReplayCase::random(&mut random, shape)
    }

    fn layered_case(seed: u64, replay_inside_layer: bool) -> ReplayCase {
        let mut random = StdRng::seed_from_u64(seed);
        let shape = ScriptShape {
            primitives: 24,
            max_layer_depth: 2,
            clip_chance: 0.35,
        };
        let mut next_id = 1;
        let view = random_script(&mut random, shape, &mut next_id);
        let mut recorded = Scene::default();
        if !replay_inside_layer {
            paint(&mut recorded, &[Op::StartLayer]);
        }
        let start = recorded.len();
        paint(&mut recorded, &view);
        let range = start..recorded.len();
        if !replay_inside_layer {
            paint(&mut recorded, &[Op::EndLayer]);
        }
        recorded.finish();

        let mut before = Vec::new();
        let mut after = Vec::new();
        if replay_inside_layer {
            before.push(Op::StartLayer);
            after.push(Op::EndLayer);
        } else {
            before.push(Op::StartLayer);
            after.push(Op::EndLayer);
        }
        ReplayCase {
            recorded,
            range,
            before,
            view,
            after,
        }
    }

    fn repeated_replay_case(seed: u64, frames: usize, replay: Replay) {
        let case = random_case(
            seed,
            ScriptShape {
                primitives: 30,
                max_layer_depth: 2,
                clip_chance: 0.45,
            },
        );
        let expected = case.rendered();
        let mut previous = case.recorded;
        let mut range = case.range;
        for frame in 0..frames {
            let mut scene = Scene::default();
            paint(&mut scene, &case.before);
            replay(&mut scene, range.clone(), &previous);
            let replayed_start = scene.len() - range.len();
            paint(&mut scene, &case.after);
            scene.finish();
            if let Some(mismatch) = draw_order_mismatch(&expected, &scene) {
                panic!("seed {seed}, frame {}: {mismatch}", frame + 1);
            }
            range = replayed_start..replayed_start + range.len();
            previous = scene;
        }
    }

    fn heavily_clipped_case(seed: u64) -> ReplayCase {
        random_case(
            seed,
            ScriptShape {
                primitives: 36,
                max_layer_depth: 3,
                clip_chance: 0.95,
            },
        )
    }

    fn all_kinds_shared_order_case(seed: u64) -> ReplayCase {
        let mut random = StdRng::seed_from_u64(seed);
        let mut next_id = 1;
        let kinds = [
            Kind::Shadow,
            Kind::Quad,
            Kind::Underline,
            Kind::MonochromeSprite,
            Kind::PolychromeSprite,
        ];
        let bounds = Bounds {
            origin: point(ScaledPixels(16.), ScaledPixels(16.)),
            size: size(ScaledPixels(128.), ScaledPixels(128.)),
        };
        let mut view = vec![Op::StartLayer];
        for kind in kinds {
            view.push(Op::Primitive {
                kind,
                id: next_id,
                bounds,
                content_mask: bounds,
                texture: random.random_range(0..3),
            });
            next_id += 1;
        }
        view.push(Op::EndLayer);
        let mut recorded = Scene::default();
        paint(&mut recorded, &view);
        let range = 0..recorded.len();
        recorded.finish();
        let before = vec![Op::Primitive {
            kind: Kind::Quad,
            id: next_id,
            bounds,
            content_mask: bounds,
            texture: 0,
        }];
        next_id += 1;
        let after = vec![Op::Primitive {
            kind: Kind::Shadow,
            id: next_id,
            bounds,
            content_mask: bounds,
            texture: 0,
        }];
        ReplayCase {
            recorded,
            range,
            before,
            view,
            after,
        }
    }

    fn animated_case(seed: u64) -> ReplayCase {
        let mut random = StdRng::seed_from_u64(seed);
        let mut next_id = 1;
        let mut view = Vec::new();
        for _ in 0..8 {
            let bounds = Bounds {
                origin: point(
                    ScaledPixels(random.random_range(0. ..128.)),
                    ScaledPixels(random.random_range(0. ..128.)),
                ),
                size: size(ScaledPixels(16.), ScaledPixels(16.)),
            };
            view.push(Op::AnimatedSprite {
                id: next_id,
                bounds,
                content_mask: bounds,
                texture: random.random_range(0..3),
            });
            next_id += 1;
        }
        let mut recorded = Scene::default();
        paint(&mut recorded, &view);
        let range = 0..recorded.len();
        recorded.finish();
        ReplayCase {
            recorded,
            range,
            before: vec![square(next_id, 0.)],
            view,
            after: vec![square(next_id + 1, 4.)],
        }
    }

    #[test]
    fn oracle_flags_overlapping_primitives_drawn_in_the_other_order() {
        let expected = finished(&[square(1, 0.), square(2, 5.)]);
        let swapped = finished(&[square(2, 5.), square(1, 0.)]);
        assert!(draw_order_mismatch(&expected, &swapped).is_some());
    }

    #[test]
    fn oracle_accepts_disjoint_primitives_in_any_order() {
        let expected = finished(&[square(1, 0.), square(2, 50.)]);
        let swapped = finished(&[square(2, 50.), square(1, 0.)]);
        assert_eq!(draw_order_mismatch(&expected, &swapped), None);
    }

    #[test]
    fn oracle_flags_a_missing_primitive() {
        let expected = finished(&[square(1, 0.), square(2, 50.)]);
        let missing = finished(&[square(1, 0.)]);
        assert!(draw_order_mismatch(&expected, &missing).is_some());
    }

    #[test]
    fn replay_oracle_equal_keys_preserve_layer_push_order() {
        let view = vec![
            Op::StartLayer,
            Op::StartLayer,
            square(1, 0.),
            Op::EndLayer,
            square(2, 5.),
            Op::EndLayer,
        ];
        let recorded = finished(&view);
        assert_eq!(recorded.quads.len(), 2);
        assert!(
            recorded
                .quads
                .windows(2)
                .all(|pair| pair[0].order == pair[1].order)
        );
        assert_eq!(
            draw_sequence(&recorded)
                .iter()
                .map(|drawn| drawn.id)
                .collect::<Vec<_>>(),
            [2, 1],
            "equal-key quads retain the outer-first layer flush order",
        );
        let case = ReplayCase {
            range: 0..recorded.len(),
            recorded,
            before: Vec::new(),
            view,
            after: Vec::new(),
        };
        for (name, replay) in REPLAYS {
            assert_case(
                &case,
                replay,
                format_args!("{name}: {}", "equal-key nested layers"),
            );
        }
    }

    #[test]
    fn replay_oracle_random_and_no_prefix_cases() {
        for seed in 0..250 {
            let mut case = random_case(
                seed,
                ScriptShape {
                    primitives: 28,
                    max_layer_depth: 2,
                    clip_chance: 0.25,
                },
            );
            if seed % 2 == 0 {
                case.before.clear();
            }
            for (name, replay) in REPLAYS {
                assert_case(
                    &case,
                    replay,
                    format_args!("{name}: {}", format_args!("seed {seed}")),
                );
            }
        }
    }

    #[test]
    fn replay_oracle_animated_sprite_cases() {
        for seed in 250..500 {
            let case = animated_case(seed);
            for (name, replay) in REPLAYS {
                assert_case(
                    &case,
                    replay,
                    format_args!("{name}: {}", format_args!("seed {seed}")),
                );
            }
        }
    }

    #[test]
    fn replay_oracle_ranges_recorded_inside_layers() {
        for seed in 500..750 {
            let case = layered_case(seed, false);
            for (name, replay) in REPLAYS {
                assert_case(
                    &case,
                    replay,
                    format_args!("{name}: {}", format_args!("seed {seed}")),
                );
            }
        }
    }

    #[test]
    fn replay_oracle_ranges_replayed_inside_layers() {
        for seed in 750..1000 {
            let case = layered_case(seed, true);
            for (name, replay) in REPLAYS {
                assert_case(
                    &case,
                    replay,
                    format_args!("{name}: {}", format_args!("seed {seed}")),
                );
            }
        }
    }

    #[test]
    fn replay_oracle_replays_of_replays() {
        for seed in 1000..1250 {
            for (_, replay) in REPLAYS {
                repeated_replay_case(seed, 2, replay);
            }
            for (_, replay) in REPLAYS {
                repeated_replay_case(seed, 3, replay);
            }
        }
    }

    #[test]
    fn replay_oracle_empty_ranges() {
        for seed in 1250..1500 {
            let mut case = random_case(
                seed,
                ScriptShape {
                    primitives: 20,
                    max_layer_depth: 1,
                    clip_chance: 0.2,
                },
            );
            case.range = 0..0;
            case.view.clear();
            for (name, replay) in REPLAYS {
                assert_case(
                    &case,
                    replay,
                    format_args!("{name}: {}", format_args!("seed {seed}")),
                );
            }
        }
    }

    #[test]
    fn replay_oracle_prefixes_covering_everything() {
        for seed in 1500..1750 {
            let mut case = all_kinds_shared_order_case(seed);
            case.before.extend(case.before.clone());
            for (name, replay) in REPLAYS {
                assert_case(
                    &case,
                    replay,
                    format_args!("{name}: {}", format_args!("seed {seed}")),
                );
            }
        }
    }

    #[test]
    fn replay_oracle_all_kinds_sharing_orderings() {
        for seed in 1750..2000 {
            let case = all_kinds_shared_order_case(seed);
            for (name, replay) in REPLAYS {
                assert_case(
                    &case,
                    replay,
                    format_args!("{name}: {}", format_args!("seed {seed}")),
                );
            }
        }
    }

    #[test]
    fn replay_oracle_heavy_clipping_cases() {
        for seed in 2000..2250 {
            let case = heavily_clipped_case(seed);
            for (name, replay) in REPLAYS {
                assert_case(
                    &case,
                    replay,
                    format_args!("{name}: {}", format_args!("seed {seed}")),
                );
            }
        }
    }
}

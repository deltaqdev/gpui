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
use crate::{AtlasTextureId, AtlasTextureKind, Hsla, TileId, point, size};
use collections::FxHashMap;
use rand::{Rng as _, rngs::StdRng};

/// Ids are encoded in a color's hue as `id / ID_SCALE`, exact in an `f32` below 2^24.
const ID_SCALE: f32 = (1 << 20) as f32;
const EXTENT: f32 = 512.;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Kind {
    Shadow,
    Quad,
    Underline,
    MonochromeSprite,
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
            PrimitiveBatch::Paths(_)
            | PrimitiveBatch::SubpixelSprites { .. }
            | PrimitiveBatch::Surfaces(_) => {
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

    /// The baseline every faster replay must meet: replaying a recorded range
    /// draws like painting it again.
    #[test]
    fn per_operation_replay_draws_like_a_full_render() {
        let shapes = [
            ScriptShape {
                primitives: 40,
                max_layer_depth: 0,
                clip_chance: 0.,
            },
            ScriptShape {
                primitives: 120,
                max_layer_depth: 3,
                clip_chance: 0.2,
            },
        ];
        for seed in 0..300 {
            let mut rng = StdRng::seed_from_u64(seed);
            let shape = shapes[seed as usize % shapes.len()];
            let case = ReplayCase::random(&mut rng, shape);
            let replayed = case.replayed(|scene, range, recorded| scene.replay(range, recorded));
            if let Some(mismatch) = draw_order_mismatch(&case.rendered(), &replayed) {
                panic!("seed {seed}: {mismatch}");
            }
        }
    }
}

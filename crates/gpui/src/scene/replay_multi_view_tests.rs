//! Randomized frames of nested cached views, each frame checked against a full
//! render with the draw-order oracle: views are cached at random, a child view's
//! range is mapped through its parent's replay, and content around them changes.

use super::replay_oracle::{Kind, Op, draw_order_mismatch, paint};
use super::*;
use crate::{point, size};
use rand::{Rng as _, SeedableRng as _, rngs::StdRng};

#[derive(Clone, Debug)]
enum Segment {
    Ops(Vec<Op>),
    View(usize),
    Layer(Vec<Segment>),
}

struct World {
    views: Vec<Vec<Segment>>,
    descendants: Vec<Vec<usize>>,
}

struct ScriptGenerator {
    extent: f32,
    max_depth: usize,
    clip_chance: f64,
    animated: f64,
}

fn random_ops(
    rng: &mut StdRng,
    count: usize,
    next_id: &mut u32,
    generator: &ScriptGenerator,
) -> Vec<Op> {
    let kinds = [
        Kind::Shadow,
        Kind::Quad,
        Kind::Underline,
        Kind::MonochromeSprite,
        Kind::SubpixelSprite,
        Kind::PolychromeSprite,
    ];
    let mut script = Vec::new();
    let mut depth = 0;
    for _ in 0..count {
        if depth < generator.max_depth && rng.random_bool(0.2) {
            script.push(Op::StartLayer);
            depth += 1;
        } else if depth > 0 && rng.random_bool(0.2) {
            script.push(Op::EndLayer);
            depth -= 1;
        }
        if rng.random_bool(0.05) {
            script.push(Op::StartLayer);
            script.push(Op::EndLayer);
        }
        let extent = generator.extent;
        let bounds = Bounds {
            origin: point(
                ScaledPixels(rng.random_range(0. ..extent)),
                ScaledPixels(rng.random_range(0. ..extent)),
            ),
            size: size(
                ScaledPixels(rng.random_range(1. ..extent / 3.)),
                ScaledPixels(rng.random_range(1. ..extent / 3.)),
            ),
        };
        let content_mask = if rng.random_bool(generator.clip_chance) {
            Bounds {
                origin: point(
                    ScaledPixels(rng.random_range(0. ..extent)),
                    ScaledPixels(rng.random_range(0. ..extent)),
                ),
                size: size(
                    ScaledPixels(rng.random_range(0. ..extent / 2.)),
                    ScaledPixels(rng.random_range(0. ..extent / 2.)),
                ),
            }
        } else {
            Bounds {
                origin: point(ScaledPixels(-1.), ScaledPixels(-1.)),
                size: size(ScaledPixels(extent * 4.), ScaledPixels(extent * 4.)),
            }
        };
        let id = *next_id;
        *next_id += 1;
        if rng.random_bool(generator.animated) {
            script.push(Op::AnimatedSprite {
                id,
                bounds,
                content_mask,
                texture: rng.random_range(0..3),
            });
        } else {
            script.push(Op::Primitive {
                kind: kinds[rng.random_range(0..kinds.len())],
                id,
                bounds,
                content_mask,
                texture: rng.random_range(0..3),
            });
        }
    }
    script.extend(std::iter::repeat_n(Op::EndLayer, depth));
    script
}

fn collect_descendants(world_views: &[Vec<Segment>], segments: &[Segment], found: &mut Vec<usize>) {
    for segment in segments {
        match segment {
            Segment::Ops(_) => {}
            Segment::View(child) => {
                found.push(*child);
                collect_descendants(world_views, &world_views[*child], found);
            }
            Segment::Layer(inner) => collect_descendants(world_views, inner, found),
        }
    }
}

fn random_world(
    rng: &mut StdRng,
    next_id: &mut u32,
    generator: &ScriptGenerator,
) -> (World, Vec<usize>) {
    let view_count = rng.random_range(2..7);
    let mut parent: Vec<Option<usize>> = vec![None; view_count];
    for child in 1..view_count {
        if rng.random_bool(0.6) {
            parent[child] = Some(rng.random_range(0..child));
        }
    }
    let mut views = Vec::new();
    for view in 0..view_count {
        let mut segments = Vec::new();
        let children: Vec<usize> = (0..view_count)
            .filter(|child| parent[*child] == Some(view))
            .collect();
        let count = rng.random_range(0..6);
        segments.push(Segment::Ops(random_ops(rng, count, next_id, generator)));
        for child in children {
            if rng.random_bool(0.2) {
                segments.push(Segment::Layer(vec![Segment::View(child)]));
            } else {
                segments.push(Segment::View(child));
            }
            let count = rng.random_range(0..5);
            segments.push(Segment::Ops(random_ops(rng, count, next_id, generator)));
        }
        if rng.random_bool(0.15) {
            views.push(vec![Segment::Layer(segments)]);
        } else {
            views.push(segments);
        }
    }
    let mut descendants = Vec::new();
    for view in 0..view_count {
        let mut found = Vec::new();
        collect_descendants(&views, &views[view], &mut found);
        descendants.push(found);
    }
    let roots = (0..view_count)
        .filter(|view| parent[*view].is_none())
        .collect();
    (World { views, descendants }, roots)
}

fn depth_at(scene: &Scene, index: usize) -> usize {
    let mut depth = 0usize;
    for operation in &scene.paint_operations[..index] {
        match operation {
            PaintOperation::StartLayer => depth += 1,
            PaintOperation::EndLayer => depth -= 1,
            PaintOperation::Primitive(_) => {}
        }
    }
    depth
}

struct Previous<'a> {
    scene: &'a Scene,
    ranges: &'a [Option<Range<usize>>],
}

fn paint_segments(
    scene: &mut Scene,
    segments: &[Segment],
    world: &World,
    cached: &[bool],
    previous: Option<&Previous>,
    ranges: &mut Vec<Option<Range<usize>>>,
) {
    for segment in segments {
        match segment {
            Segment::Ops(ops) => paint(scene, ops),
            Segment::Layer(inner) => {
                scene.push_layer();
                paint_segments(scene, inner, world, cached, previous, ranges);
                scene.pop_layer();
            }
            Segment::View(view) => {
                let view = *view;
                let start = scene.len();
                let previous_range = previous.and_then(|previous| {
                    previous.ranges[view].clone().map(|range| (previous, range))
                });
                if cached[view]
                    && let Some((previous, range)) = previous_range
                {
                    let depth = depth_at(previous.scene, range.start);
                    // Mirror Window::reuse_paint, which reads the destination depth itself.
                    scene.replay_with_one_insert(range.clone(), previous.scene, depth);
                    ranges[view] = Some(start..scene.len());
                    for descendant in &world.descendants[view] {
                        if let Some(old) = &previous.ranges[*descendant] {
                            ranges[*descendant] = Some(
                                start + old.start - range.start..start + old.end - range.start,
                            );
                        }
                    }
                } else {
                    paint_segments(scene, &world.views[view], world, cached, previous, ranges);
                    ranges[view] = Some(start..scene.len());
                }
            }
        }
    }
}

fn check_animations(scene: &Scene) -> Option<String> {
    for (index, animation) in scene.animations.iter().enumerate() {
        match scene.monochrome_sprites.get(animation.sprite_index) {
            Some(sprite) if sprite.animation.0 as usize == index => {}
            _ => return Some(format!("animation {index} points at the wrong sprite")),
        }
    }
    let animated = scene
        .monochrome_sprites
        .iter()
        .filter(|sprite| sprite.animation != SpriteAnimationId::NONE)
        .count();
    if animated != scene.animations.len() {
        return Some(format!(
            "{animated} animated sprites, {} animations",
            scene.animations.len()
        ));
    }
    None
}

fn run_seed(seed: u64, generator: &ScriptGenerator, frames: usize) {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut next_id = 1;
    let (world, roots) = random_world(&mut rng, &mut next_id, generator);
    let mut previous_scene: Option<Scene> = None;
    let mut previous_ranges: Vec<Option<Range<usize>>> = vec![None; world.views.len()];
    for frame in 0..frames {
        let mut top = Vec::new();
        for root in &roots {
            let count = rng.random_range(0..6);
            top.push(Segment::Ops(random_ops(
                &mut rng,
                count,
                &mut next_id,
                generator,
            )));
            if rng.random_bool(0.15) {
                top.push(Segment::Layer(vec![Segment::View(*root)]));
            } else {
                top.push(Segment::View(*root));
            }
        }
        let count = rng.random_range(0..6);
        top.push(Segment::Ops(random_ops(
            &mut rng,
            count,
            &mut next_id,
            generator,
        )));
        let cached: Vec<bool> = (0..world.views.len())
            .map(|_| rng.random_bool(0.7))
            .collect();

        let mut expected = Scene::default();
        let mut unused = vec![None; world.views.len()];
        paint_segments(
            &mut expected,
            &top,
            &world,
            &vec![false; world.views.len()],
            None,
            &mut unused,
        );
        expected.finish();

        let mut actual = Scene::default();
        let mut ranges = vec![None; world.views.len()];
        let previous = previous_scene.as_ref().map(|scene| Previous {
            scene,
            ranges: &previous_ranges,
        });
        paint_segments(
            &mut actual,
            &top,
            &world,
            &cached,
            previous.as_ref(),
            &mut ranges,
        );
        actual.finish();

        assert_eq!(
            expected.len(),
            actual.len(),
            "seed {seed} frame {frame}: op count"
        );
        if let Some(mismatch) = draw_order_mismatch(&expected, &actual) {
            panic!(
                "seed {seed} frame {frame}: {mismatch}\nworld {:?}\ntop {:?}\ncached {cached:?}",
                world.views, top
            );
        }
        if let Some(problem) = check_animations(&actual) {
            panic!("seed {seed} frame {frame}: {problem}");
        }
        FAST.with(|fast| fast.set(fast.get() + actual.replay_extents.len()));
        MEMO.with(|memo| {
            if let Some(previous) = previous_scene.as_ref() {
                memo.set(memo.get() + previous.replay_extents.len());
            }
        });
        previous_scene = Some(actual);
        previous_ranges = ranges;
    }
}

thread_local! {
    static FAST: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static MEMO: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[test]
fn multi_view_replays_take_the_fast_path() {
    let generator = ScriptGenerator {
        extent: 64.,
        max_depth: 2,
        clip_chance: 0.2,
        animated: 0.05,
    };
    for seed in 0..2_000 {
        run_seed(seed, &generator, 5);
    }
    eprintln!(
        "fast-path replays {} memo-available {}",
        FAST.with(|f| f.get()),
        MEMO.with(|m| m.get())
    );
}

#[test]
fn multi_view_frames_draw_like_full_renders_dense() {
    let generator = ScriptGenerator {
        extent: 64.,
        max_depth: 2,
        clip_chance: 0.2,
        animated: 0.05,
    };
    for seed in 0..2_000 {
        run_seed(seed, &generator, 5);
    }
}

#[test]
fn multi_view_frames_draw_like_full_renders_sparse() {
    let generator = ScriptGenerator {
        extent: 400.,
        max_depth: 3,
        clip_chance: 0.4,
        animated: 0.1,
    };
    for seed in 0..1_000 {
        run_seed(seed, &generator, 5);
    }
}

#[test]
fn multi_view_frames_draw_like_full_renders_flat() {
    let generator = ScriptGenerator {
        extent: 40.,
        max_depth: 0,
        clip_chance: 0.0,
        animated: 0.0,
    };
    for seed in 0..2_000 {
        run_seed(seed, &generator, 6);
    }
}

#[test]
fn a_nan_content_mask_in_a_cached_range_draws_like_a_full_render() {
    let square = Bounds {
        origin: point(ScaledPixels(0.), ScaledPixels(0.)),
        size: size(ScaledPixels(10.), ScaledPixels(10.)),
    };
    let nan = Bounds {
        origin: point(ScaledPixels(f32::NAN), ScaledPixels(f32::NAN)),
        size: size(ScaledPixels(10.), ScaledPixels(10.)),
    };
    let prefix = [Op::Primitive {
        kind: Kind::Underline,
        id: 1,
        bounds: square,
        content_mask: square,
        texture: 0,
    }];
    let view = [
        Op::Primitive {
            kind: Kind::Quad,
            id: 2,
            bounds: square,
            content_mask: square,
            texture: 0,
        },
        Op::Primitive {
            kind: Kind::Quad,
            id: 3,
            bounds: square,
            content_mask: nan,
            texture: 0,
        },
    ];
    let mut recorded = Scene::default();
    paint(&mut recorded, &view);
    recorded.finish();
    let mut expected = Scene::default();
    paint(&mut expected, &prefix);
    paint(&mut expected, &view);
    expected.finish();
    let mut per_operation = Scene::default();
    paint(&mut per_operation, &prefix);
    per_operation.replay(0..recorded.len(), &recorded);
    per_operation.finish();
    let mut one_insert = Scene::default();
    paint(&mut one_insert, &prefix);
    one_insert.replay_with_one_insert(0..recorded.len(), &recorded, 0);
    one_insert.finish();
    eprintln!(
        "per-operation: {:?}",
        draw_order_mismatch(&expected, &per_operation)
    );
    eprintln!(
        "one-insert: {:?}",
        draw_order_mismatch(&expected, &one_insert)
    );
    eprintln!(
        "orders expected quads {:?} underlines {:?}; one-insert quads {:?}",
        expected.quads.iter().map(|q| q.order).collect::<Vec<_>>(),
        expected
            .underlines
            .iter()
            .map(|u| u.order)
            .collect::<Vec<_>>(),
        one_insert.quads.iter().map(|q| q.order).collect::<Vec<_>>()
    );
}

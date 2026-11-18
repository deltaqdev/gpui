//! Replaying a cached view's scene range into the next frame (`Scene::replay`).
//!
//! A cached view paints once and then, on every frame it is not notified, copies
//! the paint operations it recorded into the new scene, between whatever is
//! painted before it (a window background, a sidebar) and after it (a popover).
//! The `text` cases are a transcript or terminal: one layer per line of glyphs
//! with an occasional quad. The `mixed` cases are seeded primitives of every
//! kind in nested layers, some clipped. Each frame replays the whole range of a
//! recorded frame that had other content around it too.
//!
//! `SceneReplay/replay` measures the replay with its surroundings, and
//! `SceneReplay/replay+finish` adds `Scene::finish`, whose sorting and batching
//! depend on the draw orders the replay assigned. `GPUI_BENCH_SCENE_STATS=1`
//! prints each case's primitive, operation and batch counts.

use std::{fmt, ops::Range, time::Duration};

use gpui::{
    AtlasTextureId, AtlasTextureKind, AtlasTile, Bounds, ContentMask, Corners, Hsla,
    MonochromeSprite, PolychromeSprite, Quad, ScaledPixels, Scene, Shadow, SpriteAnimationId,
    TileId, TransformationMatrix, Underline, point,
    private::rand::{Rng as _, SeedableRng as _, rngs::StdRng},
    size,
};

const EXTENT: f32 = 1_024.;

#[derive(Clone, Copy)]
enum Fixture {
    Text { lines: usize, glyphs: usize },
    Mixed { primitives: usize, seed: u64 },
}

impl fmt::Display for Fixture {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Fixture::Text { lines, glyphs } => write!(formatter, "text-{lines}x{glyphs}"),
            Fixture::Mixed { primitives, seed } => {
                write!(formatter, "mixed-{primitives}-seed-{seed}")
            }
        }
    }
}

fn fixtures() -> Vec<Fixture> {
    let (seeds, _) = gpui::calculate_seeds(2, &[]);
    let mut fixtures = vec![
        Fixture::Text {
            lines: 40,
            glyphs: 80,
        },
        Fixture::Text {
            lines: 150,
            glyphs: 80,
        },
        Fixture::Text {
            lines: 400,
            glyphs: 120,
        },
    ];
    for seed in seeds {
        fixtures.push(Fixture::Mixed {
            primitives: 2_000,
            seed,
        });
    }
    fixtures
}

/// How a frame replays the view's range. Faster replays are added here, so
/// each case is measured the same way for every strategy.
#[derive(Clone, Copy)]
enum Strategy {
    PerOperation,
}

impl fmt::Display for Strategy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Strategy::PerOperation => write!(formatter, "per-operation"),
        }
    }
}

const STRATEGIES: [Strategy; 1] = [Strategy::PerOperation];

fn replay(strategy: Strategy, scene: &mut Scene, range: Range<usize>, recorded: &Scene) {
    match strategy {
        Strategy::PerOperation => scene.replay(range, recorded),
    }
}

fn full_mask() -> ContentMask<ScaledPixels> {
    ContentMask {
        bounds: Bounds::new(
            point(ScaledPixels(0.), ScaledPixels(0.)),
            size(ScaledPixels(EXTENT), ScaledPixels(EXTENT)),
        ),
    }
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> Bounds<ScaledPixels> {
    Bounds::new(
        point(ScaledPixels(x), ScaledPixels(y)),
        size(ScaledPixels(width), ScaledPixels(height)),
    )
}

fn tile(index: usize, kind: AtlasTextureKind) -> AtlasTile {
    AtlasTile {
        texture_id: AtlasTextureId {
            index: (index % 3) as u32,
            kind,
        },
        tile_id: TileId(index as u32),
        padding: 0,
        bounds: Bounds::default(),
    }
}

fn quad(bounds: Bounds<ScaledPixels>, content_mask: ContentMask<ScaledPixels>) -> Quad {
    Quad {
        bounds,
        content_mask,
        background: Hsla::white().into(),
        ..Default::default()
    }
}

fn glyph(
    index: usize,
    bounds: Bounds<ScaledPixels>,
    content_mask: ContentMask<ScaledPixels>,
) -> MonochromeSprite {
    MonochromeSprite {
        order: 0,
        animation: SpriteAnimationId::NONE,
        tile: tile(index, AtlasTextureKind::Monochrome),
        bounds,
        content_mask,
        color: Hsla::black(),
        transformation: TransformationMatrix::unit(),
    }
}

/// What a window paints under the view: its background and a sidebar overlapping it.
fn paint_before(scene: &mut Scene) {
    let mask = full_mask();
    scene.insert_primitive(quad(rect(0., 0., EXTENT, EXTENT), mask));
    scene.push_layer();
    scene.insert_primitive(quad(rect(0., 0., 240., EXTENT), mask));
    for line in 0..40 {
        for glyph_index in 0..20 {
            let bounds = rect(
                8. + glyph_index as f32 * 7.,
                8. + line as f32 * 16.,
                7.,
                16.,
            );
            scene.insert_primitive(glyph(line * 20 + glyph_index, bounds, mask));
        }
    }
    scene.pop_layer();
}

/// What a window paints over the view: a popover with a shadow and a few lines.
fn paint_after(scene: &mut Scene) {
    let mask = full_mask();
    let popover = rect(320., 200., 360., 240.);
    scene.insert_primitive(Shadow {
        order: 0,
        blur_radius: ScaledPixels(12.),
        bounds: popover.dilate(ScaledPixels(24.)),
        corner_radii: Corners::default(),
        content_mask: mask,
        color: Hsla::black(),
        shape_bounds: popover,
        shape_corner_radii: Corners::all(ScaledPixels(8.)),
        inset: false.into(),
        pad: 0,
    });
    scene.insert_primitive(quad(popover, mask));
    scene.push_layer();
    for line in 0..12 {
        for glyph_index in 0..40 {
            let bounds = rect(
                332. + glyph_index as f32 * 7.,
                212. + line as f32 * 16.,
                7.,
                16.,
            );
            scene.insert_primitive(glyph(line * 40 + glyph_index, bounds, mask));
        }
    }
    scene.pop_layer();
}

fn paint_text(scene: &mut Scene, lines: usize, glyphs: usize) {
    let mask = ContentMask {
        bounds: rect(240., 0., EXTENT - 240., EXTENT),
    };
    for line in 0..lines {
        let y = (line as f32 * 16.) % EXTENT;
        if line % 10 == 0 {
            scene.insert_primitive(quad(rect(240., y, EXTENT - 240., 16.), mask));
        }
        scene.push_layer();
        for glyph_index in 0..glyphs {
            let bounds = rect(248. + glyph_index as f32 * 7., y, 7., 16.);
            scene.insert_primitive(glyph(line * glyphs + glyph_index, bounds, mask));
        }
        scene.pop_layer();
    }
}

fn paint_mixed(scene: &mut Scene, primitives: usize, seed: u64) {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut depth = 0;
    for index in 0..primitives {
        if depth < 3 && rng.random_bool(0.1) {
            scene.push_layer();
            depth += 1;
        } else if depth > 0 && rng.random_bool(0.1) {
            scene.pop_layer();
            depth -= 1;
        }
        let extent = if rng.random_bool(0.85) { 32. } else { 240. };
        let bounds = rect(
            rng.random_range(0. ..EXTENT),
            rng.random_range(0. ..EXTENT),
            rng.random_range(2. ..extent),
            rng.random_range(2. ..extent),
        );
        let mask = if rng.random_bool(0.2) {
            ContentMask {
                bounds: rect(
                    rng.random_range(0. ..EXTENT),
                    rng.random_range(0. ..EXTENT),
                    rng.random_range(0. ..EXTENT / 2.),
                    rng.random_range(0. ..EXTENT / 2.),
                ),
            }
        } else {
            full_mask()
        };
        match rng.random_range(0..5) {
            0 => scene.insert_primitive(quad(bounds, mask)),
            1 => scene.insert_primitive(Underline {
                order: 0,
                pad: 0,
                bounds,
                content_mask: mask,
                color: Hsla::black(),
                thickness: ScaledPixels(1.),
                wavy: Default::default(),
            }),
            2 => scene.insert_primitive(PolychromeSprite {
                order: 0,
                pad: 0,
                tile: tile(index, AtlasTextureKind::Polychrome),
                grayscale: false.into(),
                opacity: 1.,
                bounds,
                content_mask: mask,
                corner_radii: Corners::default(),
            }),
            _ => scene.insert_primitive(glyph(index, bounds, mask)),
        }
    }
    for _ in 0..depth {
        scene.pop_layer();
    }
}

fn paint_view(scene: &mut Scene, fixture: Fixture) {
    match fixture {
        Fixture::Text { lines, glyphs } => paint_text(scene, lines, glyphs),
        Fixture::Mixed { primitives, seed } => paint_mixed(scene, primitives, seed),
    }
}

/// The frame the view rendered in, and the range of its paint operations.
fn record(fixture: Fixture) -> (Scene, Range<usize>) {
    let mut recorded = Scene::default();
    paint_before(&mut recorded);
    let start = recorded.len();
    paint_view(&mut recorded, fixture);
    let range = start..recorded.len();
    paint_after(&mut recorded);
    recorded.finish();
    (recorded, range)
}

fn replayed_frame(
    strategy: Strategy,
    scene: &mut Scene,
    range: Range<usize>,
    recorded: &Scene,
    finish: bool,
) {
    scene.clear();
    paint_before(scene);
    replay(strategy, scene, range, recorded);
    paint_after(scene);
    if finish {
        scene.finish();
    }
}

fn bench_replay(
    criterion: &mut criterion::Criterion<gpui::BenchMeasurement>,
    group_name: &str,
    finish: bool,
) {
    let mut group = criterion.benchmark_group(group_name);
    for fixture in fixtures() {
        let mut recording = None;
        for strategy in STRATEGIES {
            let report = bench_metrics::MetricReport::new();
            group.bench_function(format!("{fixture}/{strategy}"), |bencher| {
                // Criterion calls this only for selected cases, so --list and
                // filters do not record every fixture.
                let (recorded, range) = recording.get_or_insert_with(|| record(fixture));
                let mut scene = Scene::default();
                report.iter(bencher, || {
                    replayed_frame(strategy, &mut scene, range.clone(), recorded, finish);
                    std::hint::black_box(&scene);
                });
            });
            report.print("  ");
        }
        if finish
            && std::env::var("GPUI_BENCH_SCENE_STATS").as_deref() == Ok("1")
            && let Some((recorded, range)) = &recording
        {
            for strategy in STRATEGIES {
                let mut scene = Scene::default();
                replayed_frame(strategy, &mut scene, range.clone(), recorded, true);
                eprintln!(
                    "scene stats {group_name}/{fixture}/{strategy}: replayed operations={} \
                     frame operations={} batches={}",
                    range.len(),
                    scene.len(),
                    scene.batches().count(),
                );
            }
        }
    }
    group.finish();
}

fn scene_replay(criterion: &mut criterion::Criterion<gpui::BenchMeasurement>) {
    bench_replay(criterion, "SceneReplay/replay", false);
}

fn scene_replay_finish(criterion: &mut criterion::Criterion<gpui::BenchMeasurement>) {
    bench_replay(criterion, "SceneReplay/replay+finish", true);
}

gpui::bench_group!(
    name = benches;
    config = criterion::Criterion::default()
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(1))
        .sample_size(20);
    targets = scene_replay, scene_replay_finish
);
gpui::bench_main!(benches);

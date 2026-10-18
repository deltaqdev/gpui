//! IOSurface-backed BGRA framebuffers, allocated once, written in place by a
//! producer thread and shown by three `surface` elements at once.
//!
//! This is the shape of an out-of-process renderer (a simulator, a VNC client,
//! a game): the producer owns the pixels, GPUI samples the same memory on the
//! GPU each frame, and nothing is copied or re-uploaded.
//!
//! Click the mode button to compare one buffer, which tears because the GPU
//! reads while the producer writes, with three buffers in rotation.

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("The surface_framebuffer example is only available on macOS.");
}

#[cfg(target_os = "macos")]
#[path = "example_support/fonts.rs"]
mod example_support;

#[cfg(target_os = "macos")]
fn main() {
    platform::run();
}

#[cfg(target_os = "macos")]
mod platform {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
    use std::thread;
    use std::time::{Duration, Instant};

    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::string::CFString;
    use core_video::pixel_buffer::{CVPixelBuffer, CVPixelBufferKeys, kCVPixelFormatType_32BGRA};
    use core_video::r#return::kCVReturnSuccess;
    use gpui::{
        App, Bounds, Context, ObjectFit, SharedString, Window, WindowBounds, WindowOptions, div,
        prelude::*, px, rgb, size, surface,
    };
    use gpui_platform::application;

    use crate::example_support;

    const WIDTH: usize = 640;
    const HEIGHT: usize = 400;
    const FRAME_INTERVAL: Duration = Duration::from_millis(16);

    /// The pixel buffer handed to the producer thread.
    ///
    /// CoreVideo retains and locks pixel buffers atomically, so sharing the handle is sound.
    struct SharedBuffer(CVPixelBuffer);

    unsafe impl Send for SharedBuffer {}
    unsafe impl Sync for SharedBuffer {}

    struct Producer {
        buffers: [SharedBuffer; 3],
        /// The newest finished buffer, which the UI draws next.
        front: AtomicUsize,
        /// The buffer the UI drew last and the one before it, packed as
        /// `last << 2 | previous` so the producer reads both at once.
        shown: AtomicUsize,
        triple_buffered: AtomicBool,
        frames: AtomicU64,
        running: AtomicBool,
    }

    struct FramebufferDemo {
        buffers: [CVPixelBuffer; 3],
        producer: Arc<Producer>,
        started: Instant,
    }

    impl FramebufferDemo {
        fn new() -> Self {
            let buffers = [(); 3].map(|_| framebuffer(WIDTH, HEIGHT));
            let producer = Arc::new(Producer {
                buffers: buffers.clone().map(SharedBuffer),
                front: AtomicUsize::new(0),
                shown: AtomicUsize::new(0),
                triple_buffered: AtomicBool::new(true),
                frames: AtomicU64::new(0),
                running: AtomicBool::new(true),
            });
            thread::spawn({
                let producer = producer.clone();
                move || produce(&producer)
            });
            Self {
                buffers,
                producer,
                started: Instant::now(),
            }
        }

        fn stats(&self) -> Vec<(SharedString, SharedString)> {
            let frames = self.producer.frames.load(Ordering::Relaxed);
            let seconds = self.started.elapsed().as_secs_f64().max(0.001);
            let buffer = &self.buffers[0];
            let row = buffer.get_bytes_per_row();
            let frame_bytes = row * buffer.get_height();
            let mode = if self.producer.triple_buffered.load(Ordering::Relaxed) {
                "three: shown, in flight, being written"
            } else {
                "one, written while the GPU reads it"
            };
            vec![
                (
                    "buffers".into(),
                    format!("{WIDTH} × {HEIGHT} BGRA, allocated once").into(),
                ),
                ("in use".into(), mode.into()),
                (
                    "bytes per row".into(),
                    format!("{row} ({} pixel + {} padding)", WIDTH * 4, row - WIDTH * 4).into(),
                ),
                (
                    "frames written in place".into(),
                    format!("{frames} ({:.0} per second)", frames as f64 / seconds).into(),
                ),
                (
                    "bytes GPUI copies per frame".into(),
                    "0: the GPU samples the IOSurface".into(),
                ),
                (
                    "bytes an image upload would copy".into(),
                    format!("{:.1} MB per frame", frame_bytes as f64 / 1_000_000.0).into(),
                ),
            ]
        }
    }

    /// An IOSurface-backed, Metal-compatible BGRA pixel buffer.
    fn framebuffer(width: usize, height: usize) -> CVPixelBuffer {
        let options = CFDictionary::from_CFType_pairs(&[
            (
                CFString::from(CVPixelBufferKeys::IOSurfaceProperties),
                CFDictionary::<CFString, CFString>::from_CFType_pairs(&[]).as_CFType(),
            ),
            (
                CFString::from(CVPixelBufferKeys::MetalCompatibility),
                CFBoolean::true_value().as_CFType(),
            ),
        ]);
        CVPixelBuffer::new(kCVPixelFormatType_32BGRA, width, height, Some(&options))
            .expect("failed to create an IOSurface-backed pixel buffer")
    }

    /// Writes a moving pattern until the window closes: when triple buffered,
    /// into the one buffer the UI is neither about to draw, drawing, nor still
    /// reading on the GPU (the renderer does not wait for a frame to finish);
    /// else into the buffer on screen.
    fn produce(producer: &Producer) {
        let mut tick = 0usize;
        while producer.running.load(Ordering::Relaxed) {
            let front = producer.front.load(Ordering::Acquire);
            if !producer.triple_buffered.load(Ordering::Relaxed) {
                draw(&producer.buffers[front].0, tick);
            } else {
                let shown = producer.shown.load(Ordering::Acquire);
                let busy = [front, shown >> 2, shown & 3];
                let Some(free) = (0..3).find(|index| !busy.contains(index)) else {
                    thread::sleep(Duration::from_millis(1));
                    continue;
                };
                draw(&producer.buffers[free].0, tick);
                producer.front.store(free, Ordering::Release);
            }
            producer.frames.fetch_add(1, Ordering::Relaxed);
            tick = tick.wrapping_add(1);
            thread::sleep(FRAME_INTERVAL);
        }
    }

    fn draw(buffer: &CVPixelBuffer, tick: usize) {
        let (width, height) = (buffer.get_width(), buffer.get_height());
        if buffer.lock_base_address(0) != kCVReturnSuccess {
            return;
        }
        let row = buffer.get_bytes_per_row();
        let pixels = unsafe {
            std::slice::from_raw_parts_mut(buffer.get_base_address() as *mut u8, row * height)
        };
        for y in 0..height {
            for x in 0..width {
                let at = y * row + x * 4;
                let wave = ((x + tick * 3) ^ (y + tick)) as u8;
                pixels[at..at + 4].copy_from_slice(&[
                    wave,
                    (y * 255 / height) as u8,
                    (x * 255 / width) as u8,
                    255,
                ]);
            }
        }
        buffer.unlock_base_address(0);
    }

    impl Drop for FramebufferDemo {
        fn drop(&mut self) {
            self.producer.running.store(false, Ordering::Relaxed);
        }
    }

    impl Render for FramebufferDemo {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            // The pixels change on another thread; ask for the next frame to show them.
            window.request_animation_frame();
            let front = self.producer.front.load(Ordering::Acquire);
            let last = self.producer.shown.load(Ordering::Acquire) >> 2;
            if front != last {
                self.producer
                    .shown
                    .store(front << 2 | last, Ordering::Release);
            }
            let buffer = self.buffers[front].clone();
            let triple_buffered = self.producer.triple_buffered.load(Ordering::Relaxed);
            let view = |label: &'static str, fit: ObjectFit, width: f32, height: f32| {
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_xs().text_color(rgb(0x9a9a9a)).child(label))
                    .child(
                        // Cover paints past the element's bounds, so the parent clips it.
                        div()
                            .overflow_hidden()
                            .w(px(width))
                            .h(px(height))
                            .bg(rgb(0x000000))
                            .child(surface(buffer.clone()).object_fit(fit).size_full()),
                    )
            };
            div()
                .flex()
                .flex_col()
                .gap_4()
                .p_4()
                .size_full()
                .bg(rgb(0x1e1e1e))
                .text_color(rgb(0xe0e0e0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child("IOSurfaces written in place, shown three times")
                        .child(
                            div()
                                .id("mode")
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .bg(rgb(0x3a3a3a))
                                .hover(|style| style.bg(rgb(0x4a4a4a)))
                                .cursor_pointer()
                                .child(if triple_buffered {
                                    "Triple buffered"
                                } else {
                                    "Single buffer (tears)"
                                })
                                .on_click(cx.listener(|demo, _, _, _| {
                                    demo.producer
                                        .triple_buffered
                                        .fetch_xor(true, Ordering::Relaxed);
                                })),
                        ),
                )
                .child(view("contain", ObjectFit::Contain, 480., 300.))
                .child(
                    div()
                        .flex()
                        .gap_4()
                        .child(view("cover", ObjectFit::Cover, 140., 140.))
                        .child(view("fill", ObjectFit::Fill, 220., 80.)),
                )
                .child(div().flex().flex_col().gap_1().text_sm().children(
                    self.stats().into_iter().map(|(name, value)| {
                        div()
                            .flex()
                            .gap_2()
                            .child(div().w(px(240.)).text_color(rgb(0x9a9a9a)).child(name))
                            .child(value)
                    }),
                ))
        }
    }

    pub fn run() {
        application().run(|cx: &mut App| {
            if !example_support::load_fonts(cx) {
                return;
            }
            let bounds = Bounds::centered(None, size(px(560.), px(800.)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_, cx| cx.new(|_| FramebufferDemo::new()),
            )
            .expect("failed to open the window");
            cx.activate(true);
        });
    }
}

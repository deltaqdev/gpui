use super::*;
use crate::{
    Context, IntoElement, MouseButton, Render, Styled, TestAppContext, Window, div, hsla, point, px,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

struct ReplayOrderRoot {
    under: bool,
    card: crate::Entity<ReplayOrderCard>,
    clicks: Rc<RefCell<Vec<&'static str>>>,
}

impl Render for ReplayOrderRoot {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let clicks = self.clicks.clone();
        div()
            .size_full()
            .children(self.under.then(|| {
                div()
                    .id("under")
                    .absolute()
                    .top(px(10.))
                    .left(px(10.))
                    .size(px(80.))
                    .bg(hsla(0., 1., 0.5, 1.))
                    .on_mouse_down(MouseButton::Left, move |_, _, _| {
                        clicks.borrow_mut().push("under")
                    })
            }))
            .child(
                self.card
                    .clone()
                    .cached(crate::StyleRefinement::default().size(px(100.))),
            )
            .child(
                div()
                    .absolute()
                    .top(px(60.))
                    .left(px(60.))
                    .size(px(80.))
                    .bg(hsla(0.6, 1., 0.5, 1.)),
            )
    }
}

struct ReplayOrderCard {
    renders: Rc<Cell<usize>>,
    clicks: Rc<RefCell<Vec<&'static str>>>,
}

impl Render for ReplayOrderCard {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        let clicks = self.clicks.clone();
        div()
            .size_full()
            .child(div().absolute().size(px(50.)).bg(hsla(0.3, 1., 0.5, 1.)))
            .child(
                div()
                    .id("card")
                    .absolute()
                    .top(px(25.))
                    .left(px(25.))
                    .size(px(50.))
                    .bg(hsla(0.15, 1., 0.5, 1.))
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, _| {
                        clicks.borrow_mut().push("card")
                    }),
            )
    }
}

fn draw_batches(scene: &crate::Scene) -> Vec<String> {
    scene.batches().map(|batch| format!("{batch:?}")).collect()
}

#[gpui::test]
fn cached_view_replay_keeps_draw_order_and_hit_testing(cx: &mut TestAppContext) {
    let renders = Rc::new(Cell::new(0));
    let clicks = Rc::new(RefCell::new(Vec::new()));
    let (root, cx) = cx.add_window_view({
        let renders = renders.clone();
        let clicks = clicks.clone();
        move |_, cx| ReplayOrderRoot {
            under: false,
            card: cx.new(|_| ReplayOrderCard {
                renders,
                clicks: clicks.clone(),
            }),
            clicks,
        }
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let renders_before = renders.get();
    assert!(renders_before > 0);

    root.update(cx, |root, cx| {
        root.under = true;
        cx.notify();
    });
    let replayed = cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        draw_batches(&window.rendered_frame.scene)
    });
    assert_eq!(renders.get(), renders_before, "the card is replayed");

    root.update(cx, |_, cx| cx.notify());
    let replayed_again = cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        draw_batches(&window.rendered_frame.scene)
    });
    assert_eq!(renders.get(), renders_before, "the card is replayed again");

    cx.simulate_mouse_down(
        point(px(50.), px(50.)),
        MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_down(
        point(px(15.), px(15.)),
        MouseButton::Left,
        Default::default(),
    );
    assert_eq!(*clicks.borrow(), ["card", "under"]);

    let rendered = cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        draw_batches(&window.rendered_frame.scene)
    });
    assert!(renders.get() > renders_before, "the card is rendered again");
    assert_eq!(replayed, rendered);
    assert_eq!(replayed_again, rendered);
}

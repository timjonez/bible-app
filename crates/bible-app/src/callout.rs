//! Floating, draggable callout anchored to a span in a passage view.
//!
//! Renders inside a `gtk::Overlay` over the passage so the user can drag the
//! card anywhere in the pane to uncover the verse while a leader line keeps
//! pointing to the highlighted word or mark.

use crate::layout;
use crate::passage::WORD_OPEN_TAG;
use crate::theme;
use adw::prelude::*;
use gtk::glib;
use relm4::{adw, gtk};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const EDGE_MARGIN: f64 = 12.0;
const EDGE_MARGIN_I32: i32 = 12;
const ANCHOR_GAP: f64 = 18.0;
const CORNER_INSET: f64 = 14.0;
const WORD_INSET: f64 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn left(self) -> f64 {
        self.x
    }
    pub fn right(self) -> f64 {
        self.x + self.w
    }
    pub fn top(self) -> f64 {
        self.y
    }
    pub fn bottom(self) -> f64 {
        self.y + self.h
    }
    pub fn center_x(self) -> f64 {
        self.x + self.w * 0.5
    }
    pub fn center_y(self) -> f64 {
        self.y + self.h * 0.5
    }
    pub fn inflated(self, pad: f64) -> Self {
        Self {
            x: self.x - pad,
            y: self.y - pad,
            w: self.w + pad * 2.0,
            h: self.h + pad * 2.0,
        }
    }
    pub fn overlaps(self, other: Self) -> bool {
        self.left() < other.right()
            && self.right() > other.left()
            && self.top() < other.bottom()
            && self.bottom() > other.top()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OffscreenEdge {
    Above,
    Below,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leader {
    pub start: Point,
    pub c1: Point,
    pub c2: Point,
    pub end: Point,
    pub offscreen: Option<OffscreenEdge>,
}

/// Choose an initial card position that avoids covering `anchor` whenever room allows.
pub fn place_callout(pane_w: f64, pane_h: f64, anchor: Rect, card_w: f64, card_h: f64) -> Point {
    let below_y = anchor.bottom() + ANCHOR_GAP;
    let fits_below = below_y + card_h <= pane_h - EDGE_MARGIN;
    let above_y = anchor.top() - ANCHOR_GAP - card_h;
    let fits_above = above_y >= EDGE_MARGIN;

    if fits_below {
        return clamp_pos(
            Point {
                x: anchor.center_x() - card_w * 0.5,
                y: below_y,
            },
            pane_w,
            pane_h,
            card_w,
            card_h,
        );
    }
    if fits_above {
        return clamp_pos(
            Point {
                x: anchor.center_x() - card_w * 0.5,
                y: above_y,
            },
            pane_w,
            pane_h,
            card_w,
            card_h,
        );
    }

    let right_x = anchor.right() + ANCHOR_GAP;
    let fits_right = right_x + card_w <= pane_w - EDGE_MARGIN;
    let left_x = anchor.left() - ANCHOR_GAP - card_w;
    let fits_left = left_x >= EDGE_MARGIN;

    if fits_right || fits_left {
        let space_right = pane_w - EDGE_MARGIN - anchor.right();
        let space_left = anchor.left() - EDGE_MARGIN;
        let x = if fits_right && (!fits_left || space_right >= space_left) {
            right_x
        } else {
            left_x
        };
        return clamp_pos(
            Point {
                x,
                y: anchor.center_y() - card_h * 0.5,
            },
            pane_w,
            pane_h,
            card_w,
            card_h,
        );
    }

    let space_below = pane_h - EDGE_MARGIN - anchor.bottom();
    let space_above = anchor.top() - EDGE_MARGIN;
    let y = if space_below >= space_above {
        below_y
    } else {
        above_y
    };
    clamp_pos(
        Point {
            x: anchor.center_x() - card_w * 0.5,
            y,
        },
        pane_w,
        pane_h,
        card_w,
        card_h,
    )
}

/// Keep a card position inside the visible passage pane.
pub fn clamp_pos(pos: Point, pane_w: f64, pane_h: f64, card_w: f64, card_h: f64) -> Point {
    let x = clamp_axis(pos.x, pane_w, card_w);
    let y = clamp_axis(pos.y, pane_h, card_h);
    Point { x, y }
}

fn clamp_axis(value: f64, pane: f64, size: f64) -> f64 {
    if pane >= size + EDGE_MARGIN * 2.0 {
        value.clamp(EDGE_MARGIN, pane - EDGE_MARGIN - size)
    } else {
        ((pane - size) * 0.5).max(0.0)
    }
}

/// Pick the visual line fragment of a wrapped span closest to `card`.
pub fn pick_anchor_rect(rects: &[Rect], card: Rect) -> Option<Rect> {
    let cx = card.center_x();
    let cy = card.center_y();
    rects.iter().copied().min_by(|a, b| {
        let da = (a.center_x() - cx).hypot(a.center_y() - cy);
        let db = (b.center_x() - cx).hypot(b.center_y() - cy);
        da.total_cmp(&db)
    })
}

/// Compute a smooth Bézier leader line from the nearest edge of `card` to `anchor`.
/// Returns `None` while the card overlaps the visible anchor word.
pub fn compute_leader(card: Rect, anchor: Rect, pane_w: f64, pane_h: f64) -> Option<Leader> {
    let offscreen = if anchor.bottom() < 0.0 {
        Some(OffscreenEdge::Above)
    } else if anchor.top() > pane_h {
        Some(OffscreenEdge::Below)
    } else {
        None
    };

    if let Some(edge) = offscreen {
        let end = Point {
            x: anchor
                .center_x()
                .clamp(EDGE_MARGIN, (pane_w - EDGE_MARGIN).max(EDGE_MARGIN)),
            y: match edge {
                OffscreenEdge::Above => 8.0,
                OffscreenEdge::Below => (pane_h - 8.0).max(8.0),
            },
        };
        let inset_x = CORNER_INSET.min(card.w * 0.4);
        let start_x = end.x.clamp(card.left() + inset_x, card.right() - inset_x);
        let (start_y, dir_y) = match edge {
            OffscreenEdge::Above => (card.top(), -1.0),
            OffscreenEdge::Below => (card.bottom(), 1.0),
        };
        let start = Point {
            x: start_x,
            y: start_y,
        };
        let gap = (end.y - start.y).abs();
        let handle = bezier_handle(gap);
        return Some(Leader {
            start,
            c1: Point {
                x: start.x,
                y: start.y + dir_y * handle,
            },
            c2: Point {
                x: end.x,
                y: end.y - dir_y * handle,
            },
            end,
            offscreen: Some(edge),
        });
    }

    if card.inflated(2.0).overlaps(anchor) {
        return None;
    }

    let v_gap = (anchor.top() - card.bottom())
        .max(card.top() - anchor.bottom())
        .max(0.0);
    let h_gap = (anchor.left() - card.right())
        .max(card.left() - anchor.right())
        .max(0.0);

    let inset_x = CORNER_INSET.min(card.w * 0.4);
    let inset_y = CORNER_INSET.min(card.h * 0.4);
    let word_inset = WORD_INSET.min(anchor.w * 0.4);

    if v_gap >= h_gap {
        let card_above = card.center_y() < anchor.center_y();
        let start_x = anchor
            .center_x()
            .clamp(card.left() + inset_x, card.right() - inset_x);
        let start_y = if card_above {
            card.bottom()
        } else {
            card.top()
        };
        let end_x = start_x.clamp(anchor.left() + word_inset, anchor.right() - word_inset);
        let end_y = if card_above {
            anchor.top() - 2.0
        } else {
            anchor.bottom() + 2.0
        };
        let dir_y = if card_above { 1.0 } else { -1.0 };
        let handle = bezier_handle((end_y - start_y).abs());
        Some(Leader {
            start: Point {
                x: start_x,
                y: start_y,
            },
            c1: Point {
                x: start_x,
                y: start_y + dir_y * handle,
            },
            c2: Point {
                x: end_x,
                y: end_y - dir_y * handle,
            },
            end: Point { x: end_x, y: end_y },
            offscreen: None,
        })
    } else {
        let card_left = card.center_x() < anchor.center_x();
        let above_mid = card.center_y() <= anchor.center_y();
        let rail_y = if above_mid {
            anchor.top() - 1.0
        } else {
            anchor.bottom() + 1.0
        };
        let start_x = if card_left { card.right() } else { card.left() };
        let start_y = rail_y.clamp(card.top() + inset_y, card.bottom() - inset_y);
        let corner_inset = (WORD_INSET * 2.0).min(anchor.w * 0.5);
        let end_x = if card_left {
            anchor.left() + corner_inset
        } else {
            anchor.right() - corner_inset
        };
        let end_y = rail_y;
        let dir_x = if card_left { 1.0 } else { -1.0 };
        let handle = bezier_handle((end_x - start_x).abs());
        Some(Leader {
            start: Point {
                x: start_x,
                y: start_y,
            },
            c1: Point {
                x: start_x + dir_x * handle,
                y: start_y,
            },
            c2: Point {
                x: end_x - dir_x * handle,
                y: end_y,
            },
            end: Point { x: end_x, y: end_y },
            offscreen: None,
        })
    }
}

fn bezier_handle(axis_gap: f64) -> f64 {
    if axis_gap >= 8.0 {
        (axis_gap * 0.48).clamp(4.0, 56.0)
    } else {
        axis_gap * 0.48
    }
}

struct State {
    open: bool,
    span: Option<layout::Span>,
    pos: Point,
    needs_place: bool,
    card_rect: Option<Rect>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            span: None,
            pos: Point {
                x: EDGE_MARGIN,
                y: EDGE_MARGIN,
            },
            needs_place: true,
            card_rect: None,
        }
    }
}

#[derive(Clone)]
pub struct Callout {
    pub root: gtk::Overlay,
    view: gtk::TextView,
    line_area: gtk::DrawingArea,
    card: gtk::Box,
    title: gtk::Label,
    body_slot: gtk::Box,
    grip: gtk::DrawingArea,
    state: Rc<RefCell<State>>,
}

impl Callout {
    pub fn new(
        child: &impl IsA<gtk::Widget>,
        scroll: &gtk::ScrolledWindow,
        view: &gtk::TextView,
    ) -> Self {
        install_css();

        let root = gtk::Overlay::new();
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.set_child(Some(child));

        let state = Rc::new(RefCell::new(State::default()));

        let line_area = gtk::DrawingArea::new();
        line_area.set_hexpand(true);
        line_area.set_vexpand(true);
        line_area.set_can_target(false);
        line_area.set_focusable(false);
        line_area.set_visible(false);

        let grip = drag_grip();
        let title = gtk::Label::new(None);
        title.add_css_class("caption");
        title.add_css_class("callout-title");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title.set_max_width_chars(36);
        title.set_selectable(false);
        title.set_can_target(false);

        let drag_area = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        drag_area.add_css_class("callout-drag");
        drag_area.set_hexpand(true);
        drag_area.set_valign(gtk::Align::Fill);
        drag_area.set_cursor_from_name(Some("grab"));
        drag_area.set_tooltip_text(Some("Drag to move · Double-click to reset position"));
        drag_area.append(&grip);
        drag_area.append(&title);

        let close = gtk::Button::from_icon_name("window-close-symbolic");
        close.add_css_class("flat");
        close.add_css_class("callout-close");
        close.set_valign(gtk::Align::Center);
        close.set_tooltip_text(Some("Close (Esc)"));
        close.update_property(&[gtk::accessible::Property::Label("Close")]);

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        header.add_css_class("callout-header");
        header.append(&drag_area);
        header.append(&close);

        let body_slot = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body_slot.set_hexpand(true);
        body_slot.set_vexpand(true);

        let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
        card.add_css_class("passage-callout");
        card.set_overflow(gtk::Overflow::Hidden);
        card.set_accessible_role(gtk::AccessibleRole::Dialog);
        card.set_visible(false);
        card.append(&header);
        card.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        card.append(&body_slot);

        root.add_overlay(&line_area);
        root.add_overlay(&card);

        let callout = Self {
            root,
            view: view.clone(),
            line_area,
            card,
            title,
            body_slot,
            grip,
            state,
        };
        callout.wire_layout();
        callout.wire_draw();
        callout.wire_drag(&drag_area);
        callout.wire_tracking(scroll);

        let close_callout = callout.clone();
        close.connect_clicked(move |_| {
            let callout = close_callout.clone();
            glib::idle_add_local_once(move || {
                callout.close();
                callout.view.grab_focus();
            });
        });

        let esc = gtk::EventControllerKey::new();
        esc.set_propagation_phase(gtk::PropagationPhase::Capture);
        let esc_callout = callout.clone();
        esc.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == gtk::gdk::Key::Escape && esc_callout.is_open() {
                esc_callout.close();
                esc_callout.view.grab_focus();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        callout.card.add_controller(esc);

        callout
    }

    pub fn present(&self, span: layout::Span, title: &str, content: impl IsA<gtk::Widget>) {
        self.clear_active_tag();
        while let Some(child) = self.body_slot.first_child() {
            self.body_slot.remove(&child);
        }
        self.body_slot.append(&content);
        self.title.set_text(title);
        self.card
            .update_property(&[gtk::accessible::Property::Label(title)]);

        self.apply_active_tag(span);
        {
            let mut st = self.state.borrow_mut();
            st.open = true;
            st.span = Some(span);
            st.needs_place = true;
            st.card_rect = None;
        }
        self.line_area.set_visible(true);
        self.card.set_visible(true);
        self.root.queue_allocate();
        self.line_area.queue_draw();

        // TextView layout and card measurement settle after the current cycle;
        // redraw once idle so the leader line snaps cleanly to the word.
        let line = self.line_area.clone();
        let root = self.root.clone();
        let st = self.state.clone();
        glib::idle_add_local_once(move || {
            if st.borrow().open {
                root.queue_allocate();
                line.queue_draw();
            }
        });
    }

    pub fn close(&self) {
        self.clear_active_tag();
        {
            let mut st = self.state.borrow_mut();
            if !st.open && st.span.is_none() {
                return;
            }
            st.open = false;
            st.span = None;
            st.needs_place = true;
            st.card_rect = None;
        }
        self.card.set_visible(false);
        self.line_area.set_visible(false);
        while let Some(child) = self.body_slot.first_child() {
            self.body_slot.remove(&child);
        }
    }

    pub fn is_open(&self) -> bool {
        self.state.borrow().open
    }

    pub fn is_visible_in_window(&self) -> bool {
        self.is_open() && self.root.is_mapped()
    }

    #[cfg(test)]
    pub fn active_span(&self) -> Option<layout::Span> {
        self.state.borrow().span
    }

    #[cfg(test)]
    pub fn card_position(&self) -> Option<Point> {
        let st = self.state.borrow();
        st.open.then_some(st.pos)
    }

    pub fn redraw(&self) {
        self.grip.queue_draw();
        if self.is_open() {
            self.line_area.queue_draw();
        }
    }

    /// Re-sync leader line and clamping after font size or column reflows.
    pub fn refresh(&self) {
        if !self.is_open() {
            return;
        }
        self.root.queue_allocate();
        self.line_area.queue_draw();
        let root = self.root.clone();
        let line = self.line_area.clone();
        let st = self.state.clone();
        glib::idle_add_local_once(move || {
            if st.borrow().open {
                root.queue_allocate();
                line.queue_draw();
            }
        });
    }

    /// Move the open callout to `(x, y)` clamped within the pane (used by drag and tests).
    pub fn set_position(&self, pos: Point) {
        let mut st = self.state.borrow_mut();
        if !st.open {
            return;
        }
        st.needs_place = false;
        let pane_w = f64::from(self.root.width());
        let pane_h = f64::from(self.root.height());
        if let Some(rect) = st.card_rect {
            if pane_w > 0.0 && pane_h > 0.0 {
                st.pos = clamp_pos(pos, pane_w, pane_h, rect.w, rect.h);
            } else {
                st.pos = pos;
            }
        } else {
            st.pos = pos;
        }
        drop(st);
        self.root.queue_allocate();
        self.line_area.queue_draw();
    }

    pub fn reset_position(&self) {
        let mut st = self.state.borrow_mut();
        if !st.open {
            return;
        }
        st.needs_place = true;
        drop(st);
        self.root.queue_allocate();
        self.line_area.queue_draw();
    }

    fn clear_active_tag(&self) {
        let Some(old) = self.state.borrow().span else {
            return;
        };
        let buffer = self.view.buffer();
        let total = buffer.char_count();
        let s = old.start.clamp(0, total);
        let e = old.end.clamp(s, total);
        if e > s {
            buffer.remove_tag_by_name(
                WORD_OPEN_TAG,
                &buffer.iter_at_offset(s),
                &buffer.iter_at_offset(e),
            );
        }
    }

    fn apply_active_tag(&self, span: layout::Span) {
        let buffer = self.view.buffer();
        let total = buffer.char_count();
        let s = span.start.clamp(0, total);
        let e = span.end.clamp(s, total);
        if e > s {
            buffer.apply_tag_by_name(
                WORD_OPEN_TAG,
                &buffer.iter_at_offset(s),
                &buffer.iter_at_offset(e),
            );
        }
    }

    fn wire_layout(&self) {
        let card = self.card.clone();
        let line_area = self.line_area.clone();
        let view = self.view.clone();
        let state = self.state.clone();
        self.root
            .connect_get_child_position(move |overlay, widget| {
                let pane_w = overlay.width();
                let pane_h = overlay.height();
                if pane_w <= 0 || pane_h <= 0 {
                    return None;
                }
                let ptr = widget.as_ptr();
                if ptr == line_area.upcast_ref::<gtk::Widget>().as_ptr() {
                    return Some(gtk::gdk::Rectangle::new(0, 0, pane_w, pane_h));
                }
                if ptr != card.upcast_ref::<gtk::Widget>().as_ptr() {
                    return None;
                }
                let (open, span, needs_place, saved_pos, prev_rect) = {
                    let st = state.borrow();
                    (st.open, st.span, st.needs_place, st.pos, st.card_rect)
                };
                if !open {
                    return None;
                }

                let (min_w, nat_w, _, _) = card.measure(gtk::Orientation::Horizontal, -1);
                let max_w = (pane_w - EDGE_MARGIN_I32 * 2).max(min_w);
                let w = nat_w.clamp(min_w, max_w);
                let (min_h, nat_h, _, _) = card.measure(gtk::Orientation::Vertical, w);
                let max_h = (pane_h - EDGE_MARGIN_I32 * 2).max(min_h);
                let h = nat_h.clamp(min_h, max_h);

                let (pos, settled) = if needs_place {
                    let rects = span
                        .map(|s| span_rects(&view, overlay, s))
                        .unwrap_or_default();
                    if let Some(&anchor) = rects.first() {
                        (
                            place_callout(
                                f64::from(pane_w),
                                f64::from(pane_h),
                                anchor,
                                f64::from(w),
                                f64::from(h),
                            ),
                            true,
                        )
                    } else {
                        (
                            clamp_pos(
                                Point {
                                    x: f64::from(pane_w - w) * 0.5,
                                    y: f64::from(pane_h - h) * 0.5,
                                },
                                f64::from(pane_w),
                                f64::from(pane_h),
                                f64::from(w),
                                f64::from(h),
                            ),
                            false,
                        )
                    }
                } else {
                    (
                        clamp_pos(
                            saved_pos,
                            f64::from(pane_w),
                            f64::from(pane_h),
                            f64::from(w),
                            f64::from(h),
                        ),
                        true,
                    )
                };

                let rx = pos.x.round();
                let ry = pos.y.round();
                let rect = Rect {
                    x: rx,
                    y: ry,
                    w: f64::from(w),
                    h: f64::from(h),
                };
                {
                    let mut st = state.borrow_mut();
                    st.pos = pos;
                    if settled {
                        st.needs_place = false;
                    }
                    st.card_rect = Some(rect);
                }
                if prev_rect != Some(rect) {
                    let line = line_area.clone();
                    glib::idle_add_local_once(move || line.queue_draw());
                }
                Some(gtk::gdk::Rectangle::new(rx as i32, ry as i32, w, h))
            });
    }

    fn wire_draw(&self) {
        let overlay = self.root.clone();
        let view = self.view.clone();
        let card = self.card.clone();
        let state = self.state.clone();
        self.line_area.set_draw_func(move |_, cr, width, height| {
            let (open, span, fallback_rect) = {
                let st = state.borrow();
                (st.open, st.span, st.card_rect)
            };
            let Some(span) = span.filter(|_| open) else {
                return;
            };
            let card_rect = card
                .compute_bounds(&overlay)
                .map(|b| Rect {
                    x: f64::from(b.x()),
                    y: f64::from(b.y()),
                    w: f64::from(b.width()),
                    h: f64::from(b.height()),
                })
                .filter(|r| r.w > 0.0 && r.h > 0.0)
                .or(fallback_rect);
            let Some(card_rect) = card_rect else {
                return;
            };
            let rects = span_rects(&view, &overlay, span);
            let Some(anchor) = pick_anchor_rect(&rects, card_rect) else {
                return;
            };
            let Some(leader) =
                compute_leader(card_rect, anchor, f64::from(width), f64::from(height))
            else {
                return;
            };
            draw_leader(cr, leader);
        });
    }

    fn wire_drag(&self, drag_area: &gtk::Box) {
        let drag = gtk::GestureDrag::new();
        drag.set_button(1);
        drag.set_exclusive(true);

        let origin_ptr: Rc<Cell<Option<(f64, f64)>>> = Rc::new(Cell::new(None));
        let origin_pos = Rc::new(Cell::new(Point { x: 0.0, y: 0.0 }));

        let area_begin = drag_area.clone();
        let state_begin = self.state.clone();
        let origin_ptr_begin = origin_ptr.clone();
        let origin_pos_begin = origin_pos.clone();
        drag.connect_drag_begin(move |gesture, _, _| {
            area_begin.set_cursor_from_name(Some("grabbing"));
            let st = state_begin.borrow();
            let base = st
                .card_rect
                .map(|r| Point { x: r.x, y: r.y })
                .unwrap_or(st.pos);
            origin_pos_begin.set(base);
            let ptr = gesture
                .current_event()
                .and_then(|e| e.position())
                .or_else(|| surface_pointer(&area_begin));
            origin_ptr_begin.set(ptr);
        });

        let area_update = drag_area.clone();
        let callout_update = self.clone();
        let origin_ptr_update = origin_ptr.clone();
        let origin_pos_update = origin_pos.clone();
        drag.connect_drag_update(move |gesture, offset_x, offset_y| {
            let (dx, dy) = match (
                origin_ptr_update.get(),
                gesture
                    .current_event()
                    .and_then(|e| e.position())
                    .or_else(|| surface_pointer(&area_update)),
            ) {
                (Some((ox, oy)), Some((px, py))) => (px - ox, py - oy),
                _ => (offset_x, offset_y),
            };
            if dx.abs() < 1.0 && dy.abs() < 1.0 {
                return;
            }
            gesture.set_state(gtk::EventSequenceState::Claimed);
            let base = origin_pos_update.get();
            callout_update.set_position(Point {
                x: base.x + dx,
                y: base.y + dy,
            });
        });

        let area_end = drag_area.clone();
        drag.connect_drag_end(move |_, _, _| {
            area_end.set_cursor_from_name(Some("grab"));
        });
        let area_cancel = drag_area.clone();
        drag.connect_cancel(move |_, _| {
            area_cancel.set_cursor_from_name(Some("grab"));
        });
        drag_area.add_controller(drag);

        let dbl = gtk::GestureClick::new();
        dbl.set_button(1);
        let callout_reset = self.clone();
        dbl.connect_released(move |_, n_press, _, _| {
            if n_press == 2 {
                callout_reset.reset_position();
            }
        });
        drag_area.add_controller(dbl);
    }

    fn wire_tracking(&self, scroll: &gtk::ScrolledWindow) {
        let vadj = scroll.vadjustment();
        let line_v = self.line_area.clone();
        let st_v = self.state.clone();
        vadj.connect_value_changed(move |_| {
            if st_v.borrow().open {
                line_v.queue_draw();
            }
        });
        let callout_vc = self.clone();
        vadj.connect_changed(move |_| {
            callout_vc.refresh();
        });

        let hadj = scroll.hadjustment();
        let line_h = self.line_area.clone();
        let st_h = self.state.clone();
        hadj.connect_value_changed(move |_| {
            if st_h.borrow().open {
                line_h.queue_draw();
            }
        });

        let callout_resize = self.clone();
        self.line_area.connect_resize(move |_, _, _| {
            callout_resize.refresh();
        });

        let callout_margin = self.clone();
        self.view
            .connect_notify_local(Some("left-margin"), move |_, _| {
                callout_margin.refresh();
            });
    }
}

fn drag_grip() -> gtk::DrawingArea {
    let grip = gtk::DrawingArea::new();
    grip.set_content_width(10);
    grip.set_content_height(14);
    grip.set_valign(gtk::Align::Center);
    grip.set_can_target(false);
    grip.set_focusable(false);
    grip.set_draw_func(|_, cr, w, h| {
        let palette = theme::current();
        let (r, g, b) = parse_rgb(&palette.muted);
        cr.set_source_rgba(r, g, b, 0.8);
        let cx = f64::from(w) * 0.5;
        let cy = f64::from(h) * 0.5;
        for row in [-4.0, 0.0, 4.0] {
            for col in [-2.2, 2.2] {
                cr.arc(cx + col, cy + row, 1.15, 0.0, std::f64::consts::TAU);
                let _ = cr.fill();
            }
        }
    });
    grip
}

fn draw_leader(cr: &gtk::cairo::Context, leader: Leader) {
    let palette = theme::current();
    let (ar, ag, ab) = parse_rgb(&palette.accent);
    let (br, bg, bb) = parse_rgb(&palette.background);

    let trace = |cr: &gtk::cairo::Context| {
        cr.move_to(leader.start.x, leader.start.y);
        cr.curve_to(
            leader.c1.x,
            leader.c1.y,
            leader.c2.x,
            leader.c2.y,
            leader.end.x,
            leader.end.y,
        );
    };

    // Subtle background halo keeps the connector legible where it crosses verse text.
    cr.set_line_cap(gtk::cairo::LineCap::Round);
    cr.set_line_join(gtk::cairo::LineJoin::Round);
    cr.set_dash(&[], 0.0);
    cr.set_line_width(4.2);
    cr.set_source_rgba(br, bg, bb, 0.78);
    trace(cr);
    let _ = cr.stroke();

    // Accent leader curve.
    cr.set_line_width(1.75);
    cr.set_source_rgba(ar, ag, ab, 0.92);
    if leader.offscreen.is_some() {
        cr.set_dash(&[5.0, 4.0], 0.0);
    }
    trace(cr);
    let _ = cr.stroke();
    cr.set_dash(&[], 0.0);

    // Small pin at the card border (partially tucked under the card in z-order).
    cr.set_source_rgba(ar, ag, ab, 0.92);
    cr.arc(
        leader.start.x,
        leader.start.y,
        3.0,
        0.0,
        std::f64::consts::TAU,
    );
    let _ = cr.fill();

    match leader.offscreen {
        None => {
            cr.set_source_rgba(br, bg, bb, 0.88);
            cr.arc(leader.end.x, leader.end.y, 4.8, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();

            cr.set_source_rgba(ar, ag, ab, 1.0);
            cr.arc(leader.end.x, leader.end.y, 3.2, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
        }
        Some(edge) => {
            let dir = match edge {
                OffscreenEdge::Above => -1.0,
                OffscreenEdge::Below => 1.0,
            };
            let tip_y = leader.end.y + dir * 2.5;
            let base_y = leader.end.y - dir * 4.5;
            let draw_tri = |cr: &gtk::cairo::Context| {
                cr.move_to(leader.end.x, tip_y);
                cr.line_to(leader.end.x - 4.8, base_y);
                cr.line_to(leader.end.x + 4.8, base_y);
                cr.close_path();
            };
            cr.set_line_width(2.5);
            cr.set_source_rgba(br, bg, bb, 0.85);
            draw_tri(cr);
            let _ = cr.stroke_preserve();
            cr.set_source_rgba(ar, ag, ab, 0.96);
            let _ = cr.fill();
        }
    }
}

fn span_rects(view: &gtk::TextView, overlay: &gtk::Overlay, span: layout::Span) -> Vec<Rect> {
    let buffer = view.buffer();
    let total = buffer.char_count();
    let start = span.start.clamp(0, total);
    let end = span.end.clamp(start, total);
    if end <= start {
        return Vec::new();
    }

    let mut lines: Vec<(i32, i32, i32, i32)> = Vec::new();
    for offset in start..end {
        let iter = buffer.iter_at_offset(offset);
        let loc = view.iter_location(&iter);
        let x1 = loc.x();
        let y1 = loc.y();
        let x2 = x1 + loc.width().max(1);
        let y2 = y1 + loc.height().max(1);
        if let Some(last) = lines.last_mut() {
            if y1 < last.3 && y2 > last.1 {
                last.0 = last.0.min(x1);
                last.1 = last.1.min(y1);
                last.2 = last.2.max(x2);
                last.3 = last.3.max(y2);
                continue;
            }
        }
        lines.push((x1, y1, x2, y2));
    }

    let mut rects = Vec::with_capacity(lines.len());
    for (bx1, by1, bx2, by2) in lines {
        let (wx1, wy1) = view.buffer_to_window_coords(gtk::TextWindowType::Widget, bx1, by1);
        let (wx2, wy2) = view.buffer_to_window_coords(gtk::TextWindowType::Widget, bx2, by2);
        let p1 = view.compute_point(overlay, &gtk::graphene::Point::new(wx1 as f32, wy1 as f32));
        let p2 = view.compute_point(overlay, &gtk::graphene::Point::new(wx2 as f32, wy2 as f32));
        if let (Some(p1), Some(p2)) = (p1, p2) {
            rects.push(Rect {
                x: f64::from(p1.x()),
                y: f64::from(p1.y()),
                w: f64::from(p2.x() - p1.x()).max(1.0),
                h: f64::from(p2.y() - p1.y()).max(1.0),
            });
        }
    }
    rects
}

fn surface_pointer(widget: &impl IsA<gtk::Widget>) -> Option<(f64, f64)> {
    let root = widget.as_ref().root()?;
    let surface = gtk::prelude::NativeExt::surface(&root)?;
    let pointer = surface.display().default_seat()?.pointer()?;
    let (x, y, _) = surface.device_position(&pointer)?;
    Some((x, y))
}

fn parse_rgb(hex: &str) -> (f64, f64, f64) {
    gtk::gdk::RGBA::parse(hex)
        .map(|c| {
            (
                f64::from(c.red()),
                f64::from(c.green()),
                f64::from(c.blue()),
            )
        })
        .unwrap_or((0.9, 0.65, 0.25))
}

pub fn install_css() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(
            r#"
            .passage-callout {
              background-color: @popover_bg_color;
              color: @popover_fg_color;
              border-radius: 10px;
              border: 1px solid alpha(@window_fg_color, 0.14);
              box-shadow: 0 6px 22px alpha(#000000, 0.34), 0 1px 4px alpha(#000000, 0.22);
            }
            .passage-callout .title-3 {
              line-height: normal;
            }
            .callout-header {
              min-height: 30px;
              padding: 3px 4px 3px 10px;
              background-color: alpha(@window_fg_color, 0.03);
              transition: background-color 120ms ease-out;
            }
            .callout-header:hover {
              background-color: alpha(@window_fg_color, 0.06);
            }
            .callout-title {
              font-weight: 600;
              color: alpha(@popover_fg_color, 0.85);
            }
            button.callout-close {
              min-width: 24px;
              min-height: 24px;
              padding: 0;
              border-radius: 6px;
            }
            "#,
        );
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_below_anchor_when_room_allows() {
        let anchor = Rect {
            x: 400.0,
            y: 120.0,
            w: 110.0,
            h: 24.0,
        };
        let pos = place_callout(1000.0, 800.0, anchor, 420.0, 260.0);
        assert_eq!(pos.y, anchor.bottom() + ANCHOR_GAP);
        let card = Rect {
            x: pos.x,
            y: pos.y,
            w: 420.0,
            h: 260.0,
        };
        assert!(!card.overlaps(anchor));
    }

    #[test]
    fn places_above_anchor_near_bottom_without_covering_word() {
        let anchor = Rect {
            x: 400.0,
            y: 660.0,
            w: 140.0,
            h: 24.0,
        };
        let pos = place_callout(1000.0, 740.0, anchor, 420.0, 260.0);
        assert_eq!(pos.y, anchor.top() - ANCHOR_GAP - 260.0);
        let card = Rect {
            x: pos.x,
            y: pos.y,
            w: 420.0,
            h: 260.0,
        };
        assert!(card.bottom() <= anchor.top() - ANCHOR_GAP + 0.01);
        assert!(!card.overlaps(anchor));
    }

    #[test]
    fn places_beside_anchor_when_neither_above_nor_below_fits() {
        let anchor = Rect {
            x: 320.0,
            y: 180.0,
            w: 90.0,
            h: 24.0,
        };
        let pos = place_callout(1100.0, 380.0, anchor, 420.0, 300.0);
        let card = Rect {
            x: pos.x,
            y: pos.y,
            w: 420.0,
            h: 300.0,
        };
        assert_eq!(pos.x, anchor.right() + ANCHOR_GAP);
        assert!(!card.overlaps(anchor));
    }

    #[test]
    fn clamp_keeps_card_inside_pane_including_narrow_panes() {
        let clamped = clamp_pos(Point { x: -50.0, y: 900.0 }, 800.0, 600.0, 400.0, 250.0);
        assert_eq!(clamped.x, EDGE_MARGIN);
        assert_eq!(clamped.y, 600.0 - EDGE_MARGIN - 250.0);

        // Narrower than card + 2 * margin centers cleanly without going negative.
        let narrow = clamp_pos(Point { x: 100.0, y: 100.0 }, 410.0, 260.0, 400.0, 250.0);
        assert_eq!(narrow.x, 5.0);
        assert_eq!(narrow.y, 5.0);
    }

    #[test]
    fn leader_connects_bottom_of_card_to_top_of_word_when_above() {
        let anchor = Rect {
            x: 420.0,
            y: 500.0,
            w: 120.0,
            h: 24.0,
        };
        let card = Rect {
            x: 270.0,
            y: 220.0,
            w: 420.0,
            h: 260.0,
        };
        let leader = compute_leader(card, anchor, 1000.0, 700.0).expect("leader");
        assert_eq!(leader.offscreen, None);
        assert_eq!(leader.start.y, card.bottom());
        assert_eq!(leader.end.y, anchor.top() - 2.0);
        assert_eq!(leader.start.x, anchor.center_x());
        assert_eq!(leader.end.x, anchor.center_x());
    }

    #[test]
    fn leader_switches_to_side_edge_and_interline_rail_when_dragged_into_margin() {
        let anchor = Rect {
            x: 560.0,
            y: 340.0,
            w: 120.0,
            h: 24.0,
        };
        let card = Rect {
            x: 24.0,
            y: 220.0,
            w: 420.0,
            h: 260.0,
        };
        let leader = compute_leader(card, anchor, 1200.0, 800.0).expect("leader");
        assert_eq!(leader.offscreen, None);
        assert_eq!(leader.start.x, card.right());
        assert_eq!(leader.end.x, anchor.left() + WORD_INSET * 2.0);
        // Card center (350) <= anchor center (352), so rail rides along the top inter-line gap.
        assert_eq!(leader.end.y, anchor.top() - 1.0);
        assert_eq!(leader.start.y, anchor.top() - 1.0);
    }

    #[test]
    fn leader_hides_when_card_covers_word() {
        let anchor = Rect {
            x: 400.0,
            y: 300.0,
            w: 120.0,
            h: 24.0,
        };
        let card = Rect {
            x: 350.0,
            y: 200.0,
            w: 420.0,
            h: 260.0,
        };
        assert_eq!(compute_leader(card, anchor, 1000.0, 700.0), None);
    }

    #[test]
    fn leader_indicates_offscreen_anchor_when_scrolled_away() {
        let card = Rect {
            x: 50.0,
            y: 200.0,
            w: 400.0,
            h: 260.0,
        };
        let above = Rect {
            x: 500.0,
            y: -60.0,
            w: 80.0,
            h: 24.0,
        };
        let leader = compute_leader(card, above, 1000.0, 700.0).expect("above leader");
        assert_eq!(leader.offscreen, Some(OffscreenEdge::Above));
        assert_eq!(leader.end.y, 8.0);
        assert_eq!(leader.start.y, card.top());

        let below = Rect {
            x: 500.0,
            y: 780.0,
            w: 80.0,
            h: 24.0,
        };
        let leader = compute_leader(card, below, 1000.0, 700.0).expect("below leader");
        assert_eq!(leader.offscreen, Some(OffscreenEdge::Below));
        assert_eq!(leader.end.y, 692.0);
        assert_eq!(leader.start.y, card.bottom());
    }

    #[test]
    fn pick_anchor_rect_chooses_closest_wrapped_line() {
        let line1 = Rect {
            x: 700.0,
            y: 200.0,
            w: 80.0,
            h: 24.0,
        };
        let line2 = Rect {
            x: 220.0,
            y: 230.0,
            w: 90.0,
            h: 24.0,
        };
        let card_left = Rect {
            x: 20.0,
            y: 210.0,
            w: 180.0,
            h: 160.0,
        };
        assert_eq!(pick_anchor_rect(&[line1, line2], card_left), Some(line2));
    }
}

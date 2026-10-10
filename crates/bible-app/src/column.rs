//! Centered reading column with a drag handle on each edge.
//!
//! A chapter, commentary, the Treasury, a library article, and a note share
//! one measure. While a view shares its tab with another, the handles hide
//! and the text fills its side of the split.

use crate::layout;
use adw::prelude::*;
use gtk::glib;
use relm4::{adw, gtk};
use std::cell::Cell;
use std::rc::Rc;

const HANDLE_W: i32 = 14;

pub struct Column {
    pub root: gtk::Overlay,
    view: gtk::TextView,
    preferred_px: Rc<Cell<i32>>,
    /// False while this view shares its tab with another.
    resize_on: Rc<Cell<bool>>,
    left_handle: gtk::Box,
    right_handle: gtk::Box,
}

impl Column {
    pub fn new(child: &impl IsA<gtk::Widget>, view: &gtk::TextView, column_px: i32) -> Self {
        view.set_left_margin(layout::CHAPTER_MARGIN_X);
        view.set_right_margin(layout::CHAPTER_MARGIN_X);

        let left_handle = column_handle();
        left_handle.set_halign(gtk::Align::Start);
        let right_handle = column_handle();
        right_handle.set_halign(gtk::Align::End);
        let root = gtk::Overlay::new();
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.set_child(Some(child));
        root.add_overlay(&left_handle);
        root.add_overlay(&right_handle);

        let preferred_px = Rc::new(Cell::new(column_px.clamp(1, layout::MAX_COLUMN_PX)));
        let resize_on = Rc::new(Cell::new(true));
        place_column_edges(&root, &left_handle, &right_handle, &preferred_px);

        let column = Self {
            root,
            view: view.clone(),
            preferred_px,
            resize_on,
            left_handle,
            right_handle,
        };
        column.track_pane_width();
        column.fit();
        column
    }

    pub fn set_px(&self, px: i32) {
        self.preferred_px.set(px.clamp(1, layout::MAX_COLUMN_PX));
        self.fit();
    }

    /// Edge drags apply only while this view fills its tab.
    /// Inside a split the text fills its side, and the divider resizes it.
    pub fn set_resize(&self, on: bool) {
        if self.resize_on.get() != on {
            self.resize_on.set(on);
            self.left_handle.set_visible(on);
            self.right_handle.set_visible(on);
        }
        // Refit even when the flag is unchanged. Closing a split gives the
        // view its width back after this flag was already turned on.
        self.fit();
    }

    pub fn bind(&self, sender: relm4::Sender<crate::app::Msg>) {
        bind_column_handle(
            &self.left_handle,
            -1.0,
            &self.preferred_px,
            &self.root,
            sender.clone(),
        );
        bind_column_handle(
            &self.right_handle,
            1.0,
            &self.preferred_px,
            &self.root,
            sender,
        );
    }

    fn fit(&self) {
        let pane = self.root.width();
        apply_column_insets(
            &self.view,
            pane,
            self.preferred_px.get(),
            self.resize_on.get(),
        );
        if pane > 0 {
            self.root.queue_allocate();
        }
    }

    fn track_pane_width(&self) {
        let row = self.root.clone();
        let view = self.view.clone();
        let preferred = self.preferred_px.clone();
        let resize = self.resize_on.clone();
        let seen = Rc::new(Cell::new(0));
        let seen_resize = Rc::new(Cell::new(resize.get()));
        self.root.add_tick_callback(move |_, _| {
            let pane = row.width();
            let on = resize.get();
            if pane > 0 && (pane != seen.get() || on != seen_resize.get()) {
                seen.set(pane);
                seen_resize.set(on);
                apply_column_insets(&view, pane, preferred.get(), on);
                row.queue_allocate();
            }
            glib::ControlFlow::Continue
        });
    }
}

fn apply_column_insets(view: &gtk::TextView, pane: i32, preferred: i32, resize: bool) {
    // Filling a split must drop the centering insets even when this view
    // has no width yet. Those insets are the text view's minimum, and a
    // paned will not put the divider inside them.
    if pane <= 0 && resize {
        return;
    }
    let (left, right) = layout::column_margins(pane, preferred, resize);
    view.set_left_margin(left);
    view.set_right_margin(right);
}

fn column_handle() -> gtk::Box {
    let line = gtk::Separator::new(gtk::Orientation::Vertical);
    line.add_css_class("column-edge");
    line.set_valign(gtk::Align::Fill);
    line.set_vexpand(true);
    line.set_can_target(false);

    let lead = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    lead.set_hexpand(true);
    lead.set_can_target(false);
    let trail = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    trail.set_hexpand(true);
    trail.set_can_target(false);

    let handle = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    handle.add_css_class("column-handle");
    handle.set_width_request(HANDLE_W);
    handle.set_vexpand(true);
    handle.set_valign(gtk::Align::Fill);
    handle.set_cursor_from_name(Some("ew-resize"));
    handle.set_tooltip_text(Some("Drag to set the column width"));
    handle.update_property(&[gtk::accessible::Property::Label(
        "Drag to set the column width",
    )]);
    handle.append(&lead);
    handle.append(&line);
    handle.append(&trail);
    handle
}

/// Puts each edge on the column boundary. The handles are direct overlay
/// children: a parent with can-target false is skipped by picking, so a
/// drag gesture on a child of one never runs.
fn place_column_edges(
    root: &gtk::Overlay,
    left: &gtk::Box,
    right: &gtk::Box,
    preferred: &Rc<Cell<i32>>,
) {
    let left = left.clone();
    let right = right.clone();
    let preferred = preferred.clone();
    root.connect_get_child_position(move |overlay, widget| {
        let pane = overlay.width();
        let height = overlay.height();
        if pane <= 0 || height <= 0 {
            return None;
        }
        let shown = preferred.get().clamp(1, pane);
        let side = (pane - shown) / 2;
        let ptr = widget.as_ptr();
        let edge = if ptr == left.upcast_ref::<gtk::Widget>().as_ptr() {
            side
        } else if ptr == right.upcast_ref::<gtk::Widget>().as_ptr() {
            side + shown
        } else {
            return None;
        };
        let x = (edge - HANDLE_W / 2).clamp(0, (pane - HANDLE_W).max(0));
        Some(gtk::gdk::Rectangle::new(x, 0, HANDLE_W.min(pane), height))
    });
}

fn bind_column_handle(
    handle: &gtk::Box,
    sign: f64,
    preferred: &Rc<Cell<i32>>,
    pane: &gtk::Overlay,
    sender: relm4::Sender<crate::app::Msg>,
) {
    let drag = gtk::GestureDrag::new();
    drag.set_button(1);
    drag.set_exclusive(true);
    let origin = Rc::new(Cell::new(0.0));
    let base = Rc::new(Cell::new(0));
    let moved = Rc::new(Cell::new(false));
    let preferred_begin = preferred.clone();
    let pane_begin = pane.clone();
    let handle_begin = handle.clone();
    let moved_begin = moved.clone();
    let origin_move = origin.clone();
    let base_move = base.clone();
    drag.connect_drag_begin(move |gesture, _, _| {
        moved_begin.set(false);
        let pane_w = pane_begin.width();
        let pref = preferred_begin.get();
        let shown = if pane_w > 0 {
            pref.clamp(1, pane_w)
        } else {
            pref.max(1)
        };
        base.set(shown);
        let x = gesture
            .current_event()
            .and_then(|event| event.position())
            .map(|(x, _)| x)
            .or_else(|| pointer_x(&handle_begin))
            .unwrap_or(0.0);
        origin.set(x);
    });
    let preferred_move = preferred.clone();
    let pane_move = pane.clone();
    let handle_move = handle.clone();
    let send = sender.clone();
    let moved_update = moved.clone();
    drag.connect_drag_update(move |gesture, offset_x, _| {
        let x = gesture
            .current_event()
            .and_then(|event| event.position())
            .map(|(x, _)| x)
            .or_else(|| pointer_x(&handle_move))
            .unwrap_or_else(|| origin_move.get() + offset_x);
        let delta = x - origin_move.get();
        if delta.abs() < 1.0 {
            return;
        }
        moved_update.set(true);
        let pane_w = pane_move.width();
        let limit = if pane_w > 0 {
            pane_w.min(layout::MAX_COLUMN_PX)
        } else {
            layout::MAX_COLUMN_PX
        };
        let floor = layout::MIN_COLUMN_PX.min(limit);
        let next = (base_move.get() as f64 + sign * delta).round() as i32;
        let next = next.clamp(floor, limit);
        if next != preferred_move.get() {
            send.emit(crate::app::Msg::SetColumnWidth(next));
        }
    });
    let send_end = sender;
    drag.connect_drag_end(move |_, _, _| {
        if moved.get() {
            send_end.emit(crate::app::Msg::PersistColumnWidth);
        }
    });
    handle.add_controller(drag);
}

fn pointer_x(widget: &impl gtk::prelude::IsA<gtk::Widget>) -> Option<f64> {
    let widget = widget.as_ref();
    let root = widget.root()?;
    let surface = gtk::prelude::NativeExt::surface(&root)?;
    let pointer = surface.display().default_seat()?.pointer()?;
    let (x, _, _) = surface.device_position(&pointer)?;
    Some(x)
}

pub fn install_css() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(
            r#"
            .column-handle {
              min-width: 14px;
            }
            .column-edge {
              min-width: 1px;
              background-color: alpha(@window_fg_color, 0.28);
            }
            .column-handle:hover .column-edge {
              background-color: @accent_bg_color;
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

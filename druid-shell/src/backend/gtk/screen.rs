// Copyright 2020 the Druid Authors
// SPDX-License-Identifier: Apache-2.0

//! GTK Monitors and Screen information.

use crate::kurbo::{Point, Rect, Size};
use crate::screen::Monitor;
use gtk::gdk::{Display, DisplayManager, Rectangle};

fn translate_gdk_rectangle(r: Rectangle) -> Rect {
    Rect::from_origin_size(
        Point::new(r.x() as f64, r.y() as f64),
        Size::new(r.width() as f64, r.height() as f64),
    )
}

fn translate_gdk_monitor(mon: gtk::gdk::Monitor) -> Monitor {
    let area = translate_gdk_rectangle(mon.geometry());
    Monitor::new(
        mon.is_primary(),
        area,
        translate_gdk_rectangle(mon.workarea()),
    )
}
pub(crate) fn get_mouse_position() -> (Point, Rect) {
    if !gtk::is_initialized() {
        if let Err(err) = gtk::init() {
            tracing::error!("{}", err.message);
            return (Point::ZERO, Rect::ZERO);
        }
    }

    let default_display_maybe = DisplayManager::get().default_display();
    let default_display = default_display_maybe.unwrap();
    let default_seat_maybe = default_display.default_seat();
    let default_seat = default_seat_maybe.unwrap();
    let pointer_maybe = default_seat.pointer();
    let pointer = pointer_maybe.unwrap();
    let (_, x, y) = pointer.position();

    let display = pointer.display();
    let monitor_maybe = display.monitor_at_point(x, y);
    let pointer_monitor = monitor_maybe.unwrap();
    let rect = translate_gdk_rectangle(pointer_monitor.geometry());

    return (Point::new(x.into(), y.into()), rect);
}

pub(crate) fn get_monitors() -> Vec<Monitor> {
    if !gtk::is_initialized() {
        if let Err(err) = gtk::init() {
            tracing::error!("{}", err.message);
            return Vec::new();
        }
    }
    DisplayManager::get()
        .list_displays()
        .iter()
        .flat_map(|display: &Display| {
            (0..display.n_monitors())
                .filter_map(move |i| display.monitor(i).map(translate_gdk_monitor))
        })
        .collect()
}

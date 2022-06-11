// Copyright 2020 The Druid Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! macOS Monitors and Screen information.

use crate::kurbo::Rect;
use crate::screen::Monitor;
use cocoa::appkit::NSScreen;
use cocoa::base::id;
use cocoa::foundation::{NSArray, NSPoint};
use kurbo::Point;
use objc::{class, msg_send, sel, sel_impl};

pub(crate) fn get_monitors() -> Vec<Monitor> {
    unsafe {
        let screens: id = msg_send![class![NSScreen], screens];
        let mut monitors = Vec::<(Rect, Rect)>::new();
        let mut total_rect = Rect::ZERO;

        for idx in 0..screens.count() {
            let screen = screens.objectAtIndex(idx);
            let frame = NSScreen::frame(screen);

            let frame_r = Rect::from_origin_size(
                (frame.origin.x, frame.origin.y),
                (frame.size.width, frame.size.height),
            );
            let vis_frame = NSScreen::visibleFrame(screen);
            let vis_frame_r = Rect::from_origin_size(
                (vis_frame.origin.x, vis_frame.origin.y),
                (vis_frame.size.width, vis_frame.size.height),
            );
            monitors.push((frame_r, vis_frame_r));
            total_rect = total_rect.union(frame_r)
        }
        // TODO save this total_rect.y1 for screen coord transformations in get_position/set_position
        // and invalidate on monitor changes
        transform_coords(monitors, total_rect.y1)
    }
}

fn transform_coords(monitors_build: Vec<(Rect, Rect)>, max_y: f64) -> Vec<Monitor> {
    //Flip y and move to opposite horizontal edges (On mac, Y goes up and origin is bottom left corner)
    let fix_rect = |frame: &Rect| {
        Rect::new(
            frame.x0,
            (max_y - frame.y0) - frame.height(),
            frame.x1,
            (max_y - frame.y1) + frame.height(),
        )
    };

    monitors_build
        .iter()
        .enumerate()
        .map(|(idx, (frame, vis_frame))| {
            Monitor::new(idx == 0, fix_rect(frame), fix_rect(vis_frame))
        })
        .collect()
}

#[cfg(test)]
mod test {
    use crate::backend::mac::screen::transform_coords;
    use crate::Monitor;
    use kurbo::Rect;
    use test_log::test;

    fn pair(rect: Rect) -> (Rect, Rect) {
        (rect, rect)
    }

    fn monitor(primary: bool, rect: Rect) -> Monitor {
        Monitor::new(primary, rect, rect)
    }

    #[test]
    fn test_transform_coords_1() {
        let mons = transform_coords(vec![pair(Rect::new(0., 0., 100., 100.))], 100.);

        assert_eq!(vec![monitor(true, Rect::new(0., 0., 100., 100.))], mons)
    }

    #[test]
    fn test_transform_coords_2_right() {
        let mons = transform_coords(
            vec![
                pair(Rect::new(0., 0., 100., 100.)),
                pair(Rect::new(100., 0., 200., 100.)),
            ],
            100.,
        );

        assert_eq!(
            vec![
                monitor(true, Rect::new(0., 0., 100., 100.),),
                monitor(false, Rect::new(100., 0., 200., 100.))
            ],
            mons
        )
    }

    #[test]
    fn test_transform_coords_2_up() {
        let mons = transform_coords(
            vec![
                pair(Rect::new(0., 0., 100., 100.)),
                pair(Rect::new(0., 100., 0., 200.)),
            ],
            100.,
        );

        assert_eq!(
            vec![
                monitor(true, Rect::new(0., 0., 100., 100.),),
                monitor(false, Rect::new(0., -100., 0., 0.0))
            ],
            mons
        )
    }
}

/// The current mouse location in screen coordinates.
/// Also returns monitor of the screen the cursor is in.
pub(crate) fn get_mouse_position() -> (Point, Monitor) {
    let point = unsafe {
        let location: NSPoint = msg_send![class!(NSEvent), mouseLocation];
        Point::new(location.x, location.y)
    };

    // on macOS origin is bottom left
    // (see https://developer.apple.com/library/archive/documentation/General/Conceptual/Devpedia-CocoaApp/CoordinateSystem.html)

    // find correct monitor based on x and y
    for monitor in get_monitors() {
        let rect = monitor.virtual_rect();
        let x = point.x;
        let y = point.y;

        // FYI: rect.y0, rect.y1 are already "normalized" to top-left instead of bottom-left
        if rect.x0 <= x && x <= rect.x1 &&
            rect.y0 <= y && y <= rect.y1 {
            // this is the monitor the cursor is in, now we can calculate normalized y
            // TODO: actually test this with multi-monitor setup (incl vertically stacked)
            return (Point::new(x, rect.y1 - y), monitor);
        }
    }

    println!("macos:get_mouse_position should not get here, unless there is no mouse");
    return (Point::ZERO, Monitor::new(false, Rect::ZERO, Rect::ZERO));
}

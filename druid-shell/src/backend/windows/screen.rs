// Copyright 2020 the Druid Authors
// SPDX-License-Identifier: Apache-2.0

//! Windows Monitors and Screen information.

use std::mem::size_of;
use std::ptr::null_mut;

use piet_common::kurbo::Point;
use tracing::warn;

use winapi::shared::minwindef::*;
use winapi::shared::windef::*;
use winapi::shared::winerror::*;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::winuser::*;

use crate::kurbo::Rect;
use crate::screen::Monitor;

use super::error::Error;

unsafe extern "system" fn monitorenumproc(
    hmonitor: HMONITOR,
    _hdc: HDC,
    _lprect: LPRECT,
    _lparam: LPARAM,
) -> BOOL {
    let rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        rcMonitor: rect,
        rcWork: rect,
        dwFlags: 0,
    };
    if GetMonitorInfoW(hmonitor, &mut info) == 0 {
        warn!(
            "failed to get Monitor Info: {}",
            Error::Hr(HRESULT_FROM_WIN32(GetLastError()))
        );
    };
    let primary = info.dwFlags == MONITORINFOF_PRIMARY;
    let rect = Rect::new(
        info.rcMonitor.left as f64,
        info.rcMonitor.top as f64,
        info.rcMonitor.right as f64,
        info.rcMonitor.bottom as f64,
    );
    let work_rect = Rect::new(
        info.rcWork.left as f64,
        info.rcWork.top as f64,
        info.rcWork.right as f64,
        info.rcWork.bottom as f64,
    );
    let monitors = _lparam as *mut Vec<Monitor>;
    (*monitors).push(Monitor::new(primary, rect, work_rect));
    TRUE
}

pub(crate) fn get_monitors() -> Vec<Monitor> {
    unsafe {
        let monitors = Vec::<Monitor>::new();
        let ptr = &monitors as *const Vec<Monitor>;
        if EnumDisplayMonitors(null_mut(), null_mut(), Some(monitorenumproc), ptr as isize) == 0 {
            warn!(
                "Failed to Enumerate Display Monitors: {}",
                Error::Hr(HRESULT_FROM_WIN32(GetLastError()))
            );
        };
        monitors
    }
}

/// The current mouse location in screen coordinates.
/// Also returns monitor of the screen the cursor is in.
pub(crate) fn get_mouse_position() -> (Point, Monitor) {
    let point = get_cursor_position();
    let monitor = get_monitor_at_point(point);
    let cursor_position = Point::new(point.x as f64, point.y as f64);
    return (cursor_position, monitor);
}


fn get_monitor_at_point(point: POINT) -> Monitor {
    unsafe {
        let hmonitor = MonitorFromPoint(point, MONITOR_DEFAULTTONULL);
        return hmonitor_to_monitor(hmonitor);
    }
}

fn get_cursor_position() -> POINT {
    unsafe {
        let mut pnt = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut pnt as LPPOINT) == 0 {
            warn!(
                "Failed to Get Cursor Position: {}",
                Error::Hr(HRESULT_FROM_WIN32(GetLastError()))
            );
        };

        warn!("Cursor position is x={}, y={}", pnt.x, pnt.y);

        pnt
    }
}


fn hmonitor_to_monitor(hmonitor: HMONITOR) -> Monitor {
    unsafe {
        let rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            rcMonitor: rect,
            rcWork: rect,
            dwFlags: 0,
        };
        if GetMonitorInfoW(hmonitor, &mut info) == 0 {
            warn!(
                "failed to get Monitor Info: {}",
                Error::Hr(HRESULT_FROM_WIN32(GetLastError()))
            );
        };

        let primary = info.dwFlags == MONITORINFOF_PRIMARY;
        let rect = Rect::new(
            info.rcMonitor.left as f64,
            info.rcMonitor.top as f64,
            info.rcMonitor.right as f64,
            info.rcMonitor.bottom as f64,
        );
        let work_rect = Rect::new(
            info.rcWork.left as f64,
            info.rcWork.top as f64,
            info.rcWork.right as f64,
            info.rcWork.bottom as f64,
        );

        Monitor::new(primary, rect, work_rect)
    }
}

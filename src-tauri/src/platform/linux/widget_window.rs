use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use tauri::{AppHandle, Runtime};

// Inspect the actual GTK backend: a Wayland session can still run an X11 client.
pub(crate) fn supports_global_coordinates<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.display_handle()
        .map(|handle| is_x11_display(handle.as_raw()))
        .unwrap_or(false)
}

fn is_x11_display(display: RawDisplayHandle) -> bool {
    matches!(
        display,
        RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use raw_window_handle::{WaylandDisplayHandle, XcbDisplayHandle, XlibDisplayHandle};

    #[test]
    fn coordinates_require_an_actual_x11_backend() {
        assert!(is_x11_display(RawDisplayHandle::Xlib(
            XlibDisplayHandle::new(None, 0)
        )));
        assert!(is_x11_display(RawDisplayHandle::Xcb(
            XcbDisplayHandle::new(None, 0)
        )));
        assert!(!is_x11_display(RawDisplayHandle::Wayland(
            WaylandDisplayHandle::new(std::ptr::NonNull::dangling(),)
        )));
    }
}

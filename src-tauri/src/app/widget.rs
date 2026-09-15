use crate::app::state::{WidgetShowCompletion, WidgetWindowLifecycleState};
use crate::domain::widget::{WidgetPlacement, WidgetSide};
use crate::engine::widget as widget_engine;
use crate::platform::storage_paths;
use std::time::Duration;
use tauri::{
    AppHandle, Emitter, Manager, Monitor, PhysicalPosition, PhysicalSize, Position, Runtime, Size,
    WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

pub(crate) const WIDGET_WINDOW_LABEL: &str = "widget";
pub(crate) const WIDGET_RUNTIME_COLLAPSED_EVENT: &str = "widget-runtime-collapsed";
pub(crate) const WIDGET_RUNTIME_SHOWN_EVENT: &str = "widget-runtime-shown";
const WIDGET_TITLE: &str = "Patina Widget";
const WIDGET_EXPANDED_WIDTH_WITH_OBJECT: u32 = 228;
const WIDGET_EXPANDED_WIDTH_COMPACT: u32 = 184;
const WIDGET_EXPANDED_HEIGHT: u32 = 48;
const WIDGET_COLLAPSED_WIDTH: u32 = 64;
const WIDGET_COLLAPSED_HEIGHT: u32 = 48;
const WIDGET_COLLAPSED_VISIBLE_WIDTH: u32 = 64;
const WIDGET_DESTROY_AFTER_IDLE_SECS: u64 = 5 * 60;

#[cfg(test)]
pub(super) static CANCEL_NEXT_CREATION: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WidgetWindowBounds {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

pub(crate) async fn show_widget_window<R: Runtime + 'static>(
    app: &AppHandle<R>,
    preferred_monitor: Option<Monitor>,
) -> Result<(), String> {
    let placement = widget_engine::load_widget_placement(app).await?;
    apply_widget_layout_internal(app, preferred_monitor, placement, false, false, false).await
}

pub(crate) async fn apply_widget_layout<R: Runtime + 'static>(
    app: &AppHandle<R>,
    placement: WidgetPlacement,
    expanded: bool,
    show_object_slot: bool,
) -> Result<(), String> {
    if is_main_window_visible(app) {
        close_widget_window(app);
        return Ok(());
    }

    widget_engine::save_widget_placement(app, placement).await?;
    apply_widget_layout_internal(app, None, placement, expanded, expanded, show_object_slot).await
}

pub(crate) async fn set_widget_window_expanded<R: Runtime + 'static>(
    app: &AppHandle<R>,
    expanded: bool,
    show_object_slot: bool,
) -> Result<(), String> {
    let placement = widget_engine::load_widget_placement(app).await?;
    apply_widget_layout_internal(app, None, placement, expanded, expanded, show_object_slot).await
}

pub(crate) fn close_widget_window<R: Runtime + 'static>(app: &AppHandle<R>) {
    let hide_generation = app.state::<WidgetWindowLifecycleState>().hide();
    if let Some(window) = app.get_webview_window(WIDGET_WINDOW_LABEL) {
        emit_widget_runtime_collapsed(app);
        park_widget_window(&window);
        schedule_widget_destroy_after_idle(app.clone(), hide_generation);
    }
}

fn schedule_widget_destroy_after_idle<R: Runtime + 'static>(
    app: AppHandle<R>,
    hide_generation: u64,
) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(WIDGET_DESTROY_AFTER_IDLE_SECS)).await;

        let lifecycle = app.state::<WidgetWindowLifecycleState>();
        if !lifecycle.should_destroy_hidden_window(hide_generation) {
            return;
        }

        let Some(window) = app.get_webview_window(WIDGET_WINDOW_LABEL) else {
            return;
        };

        if let Err(error) = window.destroy() {
            eprintln!("[widget] failed to destroy idle widget window: {error}");
        }
    });
}

fn emit_widget_runtime_collapsed<R: Runtime>(app: &AppHandle<R>) {
    let _ = app.emit(WIDGET_RUNTIME_COLLAPSED_EVENT, ());
}

fn emit_widget_runtime_shown<R: Runtime>(app: &AppHandle<R>) {
    let _ = app.emit(WIDGET_RUNTIME_SHOWN_EVENT, ());
}

fn park_widget_window<R: Runtime>(window: &WebviewWindow<R>) {
    let _ = window.hide();
    let _ = window.set_focusable(false);
    let _ = window.set_always_on_top(false);
    // Hidden windows receive no input. On Wayland a never-shown widget may
    // have no GDK window yet; enabling cursor passthrough can panic in Tao.
    let _ = window.set_size(Size::Physical(PhysicalSize::new(1, 1)));
    let _ = window.set_position(Position::Physical(PhysicalPosition::new(-32_000, -32_000)));
}

fn is_main_window_visible<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.get_webview_window(crate::app::tray::MAIN_WINDOW_LABEL)
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false)
}

pub(crate) fn resolve_widget_monitor<R: Runtime>(
    app: &AppHandle<R>,
    preferred_monitor: Option<Monitor>,
) -> Result<Monitor, String> {
    preferred_monitor
        .or_else(|| {
            app.get_webview_window(crate::app::tray::MAIN_WINDOW_LABEL)
                .and_then(|window| window.current_monitor().ok().flatten())
        })
        .or_else(|| app.primary_monitor().ok().flatten())
        .ok_or_else(|| "failed to resolve widget monitor".to_string())
}

fn apply_widget_bounds<R: Runtime>(
    window: &WebviewWindow<R>,
    bounds: WidgetWindowBounds,
) -> Result<(), String> {
    let size = PhysicalSize::new(bounds.width, bounds.height);
    if window.inner_size().ok() != Some(size) {
        window
            .set_size(Size::Physical(size))
            .map_err(|error| format!("failed to size widget window: {error}"))?;
    }
    let position = PhysicalPosition::new(bounds.x, bounds.y);
    if window.outer_position().ok() != Some(position) {
        window
            .set_position(Position::Physical(position))
            .map_err(|error| format!("failed to position widget window: {error}"))?;
    }
    Ok(())
}

async fn apply_widget_layout_internal<R: Runtime + 'static>(
    app: &AppHandle<R>,
    preferred_monitor: Option<Monitor>,
    placement: WidgetPlacement,
    expanded: bool,
    focus_after_show: bool,
    show_object_slot: bool,
) -> Result<(), String> {
    if is_main_window_visible(app) {
        close_widget_window(app);
        return Ok(());
    }

    let monitor = resolve_widget_monitor(app, preferred_monitor.clone()).ok();
    let lifecycle = app.state::<WidgetWindowLifecycleState>();

    if let Some(window) = app.get_webview_window(WIDGET_WINDOW_LABEL) {
        let monitor = monitor.ok_or_else(|| "failed to resolve widget monitor".to_string())?;
        let bounds = resolve_widget_bounds(&monitor, placement, expanded, show_object_slot);
        let was_visible = window.is_visible().unwrap_or(false);
        lifecycle.show_existing();
        if !was_visible {
            let _ = window.set_always_on_top(true);
            let _ = window.set_focusable(true);
        }
        if !expanded {
            emit_widget_runtime_collapsed(app);
        }
        apply_widget_bounds(&window, bounds)?;
        if !was_visible {
            window
                .show()
                .map_err(|error| format!("failed to show widget window: {error}"))?;
        }
        if focus_after_show {
            let _ = window.set_focus();
        }
        emit_widget_runtime_shown(app);
        return Ok(());
    }

    let initial_bounds = monitor
        .as_ref()
        .map(|monitor| resolve_widget_bounds(monitor, placement, expanded, show_object_slot));
    let webview_root = storage_paths::resolve_storage_paths(app)?.webview_root;
    if !lifecycle.begin_show() {
        return Ok(());
    }

    #[cfg(test)]
    if CANCEL_NEXT_CREATION.swap(false, std::sync::atomic::Ordering::SeqCst) {
        // Deterministically exercise close before the native window is registered.
        assert!(app.get_webview_window(WIDGET_WINDOW_LABEL).is_none());
        close_widget_window(app);
    }

    let window = {
        let mut builder = WebviewWindowBuilder::new(
            app,
            WIDGET_WINDOW_LABEL,
            WebviewUrl::App("index.html".into()),
        )
        .title(WIDGET_TITLE)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .closable(false)
        .decorations(false)
        .shadow(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .focusable(true)
        .focused(false)
        .visible(false)
        .data_directory(webview_root);

        if let (Some(monitor), Some(bounds)) = (monitor.as_ref(), initial_bounds) {
            builder = builder
                .position(
                    f64::from(bounds.x) / monitor.scale_factor(),
                    f64::from(bounds.y) / monitor.scale_factor(),
                )
                .inner_size(
                    f64::from(bounds.width) / monitor.scale_factor(),
                    f64::from(bounds.height) / monitor.scale_factor(),
                );
        } else {
            // Wayland may not expose a primary monitor before the first native
            // window is mapped. Register an invisible one-pixel surface first.
            builder = builder.inner_size(1.0, 1.0);
        }

        builder.build().map_err(|error| {
            let _ = lifecycle.finish_show();
            format!("failed to create widget window: {error}")
        })?
    };

    if let WidgetShowCompletion::Hidden { hide_generation } = lifecycle.finish_show() {
        park_widget_window(&window);
        schedule_widget_destroy_after_idle(app.clone(), hide_generation);
        return Ok(());
    }

    let monitor = match monitor {
        Some(monitor) => monitor,
        None => {
            match resolve_widget_monitor_after_creation(app, &window, preferred_monitor).await {
                Ok(monitor) => monitor,
                Err(error) => {
                    let hide_generation = lifecycle.hide();
                    park_widget_window(&window);
                    schedule_widget_destroy_after_idle(app.clone(), hide_generation);
                    return Err(error);
                }
            }
        }
    };
    let bounds = resolve_widget_bounds(&monitor, placement, expanded, show_object_slot);
    let _ = window.set_ignore_cursor_events(false);
    let _ = window.set_focusable(true);
    let _ = window.set_shadow(false);
    apply_widget_bounds(&window, bounds)?;
    window
        .show()
        .map_err(|error| format!("failed to show widget window: {error}"))?;
    if focus_after_show {
        let _ = window.set_focus();
    }
    emit_widget_runtime_shown(app);
    Ok(())
}

async fn resolve_widget_monitor_after_creation<R: Runtime>(
    app: &AppHandle<R>,
    window: &WebviewWindow<R>,
    preferred_monitor: Option<Monitor>,
) -> Result<Monitor, String> {
    window
        .show()
        .map_err(|error| format!("failed to map widget window for monitor discovery: {error}"))?;

    for _ in 0..40 {
        if let Some(monitor) = preferred_monitor
            .clone()
            .or_else(|| window.current_monitor().ok().flatten())
            .or_else(|| app.primary_monitor().ok().flatten())
        {
            return Ok(monitor);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    Err("failed to resolve widget monitor after native window creation".to_string())
}

fn resolve_widget_bounds(
    monitor: &Monitor,
    placement: WidgetPlacement,
    expanded: bool,
    show_object_slot: bool,
) -> WidgetWindowBounds {
    let (width, height) = resolve_widget_dimensions(expanded, show_object_slot);
    let work_area = monitor.work_area();
    resolve_widget_bounds_from_work_area(
        work_area.position.x,
        work_area.position.y,
        work_area.size.width,
        work_area.size.height,
        placement,
        width,
        height,
    )
}

fn resolve_widget_dimensions(expanded: bool, show_object_slot: bool) -> (u32, u32) {
    if expanded {
        (
            if show_object_slot {
                WIDGET_EXPANDED_WIDTH_WITH_OBJECT
            } else {
                WIDGET_EXPANDED_WIDTH_COMPACT
            },
            WIDGET_EXPANDED_HEIGHT,
        )
    } else {
        (WIDGET_COLLAPSED_WIDTH, WIDGET_COLLAPSED_HEIGHT)
    }
}

fn resolve_widget_bounds_from_work_area(
    work_x: i32,
    work_y: i32,
    work_width: u32,
    work_height: u32,
    placement: WidgetPlacement,
    width: u32,
    height: u32,
) -> WidgetWindowBounds {
    let max_y_offset = work_height.saturating_sub(height);
    let y_offset = (placement.anchor_y * f64::from(max_y_offset)).round() as i32;
    let y = work_y + y_offset;
    let collapsed_hidden_offset = if width == WIDGET_COLLAPSED_WIDTH {
        (width.saturating_sub(WIDGET_COLLAPSED_VISIBLE_WIDTH)) as i32
    } else {
        0
    };
    let x = match placement.side {
        WidgetSide::Left => work_x - collapsed_hidden_offset,
        WidgetSide::Right => work_x + work_width as i32 - width as i32 + collapsed_hidden_offset,
    };

    WidgetWindowBounds {
        x,
        y,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        resolve_widget_bounds_from_work_area, WidgetWindowBounds, WIDGET_COLLAPSED_HEIGHT,
        WIDGET_COLLAPSED_WIDTH, WIDGET_EXPANDED_HEIGHT, WIDGET_EXPANDED_WIDTH_COMPACT,
        WIDGET_EXPANDED_WIDTH_WITH_OBJECT,
    };
    use crate::domain::widget::{WidgetPlacement, WidgetSide};

    #[test]
    fn widget_bounds_snap_to_expected_collapsed_edge_and_height() {
        let left = resolve_widget_bounds_from_work_area(
            0,
            0,
            1920,
            1040,
            WidgetPlacement::new(WidgetSide::Left, 0.5),
            WIDGET_COLLAPSED_WIDTH,
            WIDGET_COLLAPSED_HEIGHT,
        );
        assert_eq!(
            left,
            WidgetWindowBounds {
                x: 0,
                y: 496,
                width: WIDGET_COLLAPSED_WIDTH,
                height: WIDGET_COLLAPSED_HEIGHT,
            }
        );

        let right = resolve_widget_bounds_from_work_area(
            0,
            0,
            1920,
            1040,
            WidgetPlacement::new(WidgetSide::Right, 0.0),
            WIDGET_COLLAPSED_WIDTH,
            WIDGET_COLLAPSED_HEIGHT,
        );
        assert_eq!(right.x, 1856);
        assert_eq!(right.y, 0);
    }

    #[test]
    fn widget_bounds_snap_to_expected_expanded_edge_and_height() {
        let left = resolve_widget_bounds_from_work_area(
            0,
            0,
            1920,
            1040,
            WidgetPlacement::new(WidgetSide::Left, 0.5),
            WIDGET_EXPANDED_WIDTH_WITH_OBJECT,
            WIDGET_EXPANDED_HEIGHT,
        );
        assert_eq!(
            left,
            WidgetWindowBounds {
                x: 0,
                y: 496,
                width: WIDGET_EXPANDED_WIDTH_WITH_OBJECT,
                height: WIDGET_EXPANDED_HEIGHT,
            }
        );

        let right = resolve_widget_bounds_from_work_area(
            0,
            0,
            1920,
            1040,
            WidgetPlacement::new(WidgetSide::Right, 0.0),
            WIDGET_EXPANDED_WIDTH_WITH_OBJECT,
            WIDGET_EXPANDED_HEIGHT,
        );
        assert_eq!(right.x, 1692);
        assert_eq!(right.y, 0);
    }

    #[test]
    fn widget_bounds_snap_to_expected_compact_expanded_width() {
        let right = resolve_widget_bounds_from_work_area(
            0,
            0,
            1920,
            1040,
            WidgetPlacement::new(WidgetSide::Right, 0.0),
            WIDGET_EXPANDED_WIDTH_COMPACT,
            WIDGET_EXPANDED_HEIGHT,
        );

        assert_eq!(right.x, 1736);
        assert_eq!(right.y, 0);
        assert_eq!(right.width, WIDGET_EXPANDED_WIDTH_COMPACT);
    }
}

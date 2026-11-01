// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Tauri v2 + Rust Proof-of-Concept: System tray integration.

use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager,
};

pub fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show_i = MenuItem::with_id(app, "show", "Open GlideDesk…", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit GlideDesk", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

    let tray_icon = if let Ok(rgba) = image::load_from_memory(include_bytes!("../../ui/assets/trayTemplate@2x.png"))
        .map(|img| img.to_rgba8())
    {
        let (w, h) = rgba.dimensions();
        Image::new_owned(rgba.into_raw(), w, h)
    } else {
        app.default_window_icon().unwrap().clone()
    };

    let _tray = TrayIconBuilder::new()
        .icon(tray_icon)
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}

// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Unit tests for phone, camera, and parity modules

#[cfg(test)]
mod tests {
    #[test]
    fn test_phone_assets_default_shape() {
        let assets = crate::phone::get_phone_assets(None);
        assert!(assets.get("apps").is_some());
        assert!(assets.get("wallpaper").is_some());
    }

    #[test]
    fn test_parity_list_apps() {
        let apps = crate::parity::list_apps().unwrap();
        assert!(apps.is_empty());
    }

    #[test]
    fn test_camera_commands_execute() {
        let _cam_mgr = crate::camera::CameraManager::new();
        assert!(crate::camera::remove_vdd().is_ok());
    }
}

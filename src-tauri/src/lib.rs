mod commands;
mod logging;
mod warp;
#[cfg(target_os = "macos")]
mod macos_titlebar;
mod network_policy;
mod plugins;
#[cfg(target_os = "windows")]
mod windows_login;
#[cfg(target_os = "android")]
mod android_login;
#[cfg(target_os = "android")]
mod android_documents;
use tauri::Manager;
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(target_os = "android")]
    let builder = builder.plugin(tauri::plugin::Builder::<tauri::Wry, ()>::new("web-login")
        .setup(|app, api| {
            let handle = api.register_android_plugin("com.hghub.player", "LoginWebViewPlugin")?;
            app.manage(android_login::AndroidLogin(handle));
            Ok(())
        }).build());
    #[cfg(target_os = "android")]
    let builder = builder.plugin(tauri::plugin::Builder::<tauri::Wry, ()>::new("plugin-documents")
        .setup(|app, api| {
            let handle = api.register_android_plugin("com.hghub.player", "PluginDocumentsPlugin")?;
            app.manage(android_documents::AndroidDocuments(handle));
            Ok(())
        }).build());
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(
            tauri::plugin::Builder::<tauri::Wry, ()>::new("orientation")
                .setup(|app, api| {
                    #[cfg(target_os = "android")]
                    {
                        let handle = api.register_android_plugin("com.hghub.player", "OrientationPlugin")?;
                        app.manage(commands::OrientationPlugin(handle));
                    }
                    #[cfg(not(target_os = "android"))]
                    let _ = (app, api);
                    Ok(())
                })
                .build(),
        )
        .plugin(
            tauri_plugin_opener::Builder::new()
                .open_js_links_on_click(false)
                .build(),
        )
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            network_policy::bind_security(&dir);
            hghub_hpp::proxy::bind(&dir);
            warp::restore(app.handle().clone());
            app.manage(plugins::PluginHost::new(dir.join("hpp")));
            // The WebView registers its playback capabilities before the first
            // plugin command; eager initialization would expose an incorrect env.
            #[cfg(target_os = "macos")]
            macos_titlebar::create_main_window(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            plugins::hpp_manage,
            plugins::pick_plugin_files,
            plugins::pick_plugin_directory,
            commands::app_boot,
            commands::security_set,
            commands::set_orientation,
            commands::debug_log,
            commands::open_url,
            commands::open_external_media,
            warp::warp_status,
            warp::warp_enable,
            warp::warp_disable
        ])
        .build(tauri::generate_context!())
        .expect("error while building HGhub")
        .run(|_app, event| {
            if matches!(event, tauri::RunEvent::Exit) { warp::exit(); }
        });
}

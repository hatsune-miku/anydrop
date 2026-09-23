//! Run on a macOS desktop: cargo run -p anydrop-desktop-tauri --example receive_activation_probe
//! Exercises real Tao windows without starting AnyDrop services or changing settings.
#[cfg(target_os = "macos")]
#[path = "../src/receive_window_macos.rs"]
mod receive_window_macos;

#[cfg(target_os = "macos")]
fn main() {
    use objc2::{class, msg_send, runtime::AnyObject, runtime::Bool};
    use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};

    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    let app = tauri::Builder::default()
        .setup(|app| {
            for label in ["main", "receive"] {
                let window = WebviewWindowBuilder::new(
                    app,
                    label,
                    WebviewUrl::External("about:blank".parse().unwrap()),
                )
                .title("AnyDrop activation probe")
                .inner_size(300.0, 120.0)
                .visible(false)
                .focused(false)
                .accept_first_mouse(true)
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .always_on_top(label == "receive")
                .build()?;
                if label == "receive" {
                    let original = unsafe { &*window.ns_window()?.cast::<AnyObject>() }.class();
                    receive_window_macos::configure(&window)?;
                    // Configuration must also be safe to repeat.
                    receive_window_macos::configure(&window)?;
                    assert_eq!(
                        unsafe { &*window.ns_window()?.cast::<AnyObject>() }.class(),
                        original
                    );
                }
            }
            Ok(())
        })
        .build(context)
        .expect("build native probe");
    app.run(|app, event| {
        if !matches!(event, RunEvent::Ready) {
            return;
        }
        let app = app.clone();
        std::thread::spawn(move || {
            // Allow applicationDidFinishLaunching and Tao's activation pass to finish.
            std::thread::sleep(std::time::Duration::from_millis(300));
            let handle = app.clone();
            app.run_on_main_thread(move || unsafe {
                let popup = handle.get_webview_window("receive").unwrap();
                let main = handle.get_webview_window("main").unwrap();
                let native = &*popup.ns_window().unwrap().cast::<AnyObject>();
                let key: Bool = msg_send![native, canBecomeKeyWindow];
                let primary: Bool = msg_send![native, canBecomeMainWindow];
                let independent: Bool = msg_send![native, _isNonactivatingPanel];
                assert!(!key.as_bool() && !primary.as_bool() && independent.as_bool());
                // Tao still accesses its original ivars after installing the receipt hooks.
                popup.set_focusable(false).unwrap();
                let key: Bool = msg_send![native, canBecomeKeyWindow];
                assert!(!key.as_bool());
                let native_main = &*main.ns_window().unwrap().cast::<AnyObject>();
                let main_can_focus: Bool = msg_send![native_main, canBecomeKeyWindow];
                let main_independent: Bool = msg_send![native_main, _isNonactivatingPanel];
                assert!(!main_independent.as_bool(), "main activation must remain unchanged");
                assert!(main_can_focus.as_bool(), "main window must remain unmodified");
                let application: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
                let _: () = msg_send![application, deactivate];
            })
            .unwrap();
            std::thread::sleep(std::time::Duration::from_millis(300));
            let handle = app.clone();
            app.run_on_main_thread(move || unsafe {
                let popup = handle.get_webview_window("receive").unwrap();
                let main = handle.get_webview_window("main").unwrap();
                let application: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
                let active: Bool = msg_send![application, isActive];
                assert!(!active.as_bool(), "probe must start in the background");
                for _ in 0..30 {
                    popup.show().unwrap();
                    assert!(popup.is_visible().unwrap());
                    assert!(!popup.is_focused().unwrap());
                    assert!(!main.is_visible().unwrap());
                    let active: Bool = msg_send![application, isActive];
                    assert!(!active.as_bool(), "show must not activate AnyDrop");
                    popup.hide().unwrap();
                    assert!(!popup.is_visible().unwrap());
                    assert!(!main.is_visible().unwrap());
                    let active: Bool = msg_send![application, isActive];
                    assert!(!active.as_bool(), "dismiss must not activate AnyDrop");
                }
                // The regular main window must still focus normally. Dismissing
                // a receipt must preserve that focus when AnyDrop is already active.
                main.show().unwrap();
                main.set_focus().unwrap();
            })
            .unwrap();
            // App activation is asynchronous; wait for the native event loop.
            std::thread::sleep(std::time::Duration::from_millis(300));
            let handle = app.clone();
            app.run_on_main_thread(move || unsafe {
                let popup = handle.get_webview_window("receive").unwrap();
                let main = handle.get_webview_window("main").unwrap();
                let application: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
                assert!(main.is_focused().unwrap());
                popup.show().unwrap();
                assert!(main.is_focused().unwrap());
                assert!(!popup.is_focused().unwrap());
                popup.hide().unwrap();
                assert!(main.is_focused().unwrap());
                main.hide().unwrap();
                let _: () = msg_send![application, deactivate];
                println!("PASS: 30 native show/hide cycles kept the app inactive and main hidden; active main-window focus, Tao ivars and KVO class preserved");
                handle.exit(0);
            })
            .unwrap();
        });
    });
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("This probe requires a macOS desktop session.");
}

use std::process::{Child, Command};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tauri::{
    Manager, WebviewUrl, WebviewWindowBuilder,
    webview::{NewWindowFeatures, NewWindowResponse},
};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

const MARIMO_PORT: u16 = 2730;
const MARIMO_URL: &str = "http://localhost:2730";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

static WINDOW_COUNTER: AtomicUsize = AtomicUsize::new(2);

struct MarimoProcess(Mutex<Option<Child>>);

// Custom title bar injected into the main window.
// Replaces native decorations: provides dragging, window controls, and the
// "← Home" button (visible only when a notebook is open, not on home page).
//
// In dev builds, loads dist/titlebar.js from the local file server via
// synchronous XHR so edits to that file take effect without recompiling Rust.
// In release builds, the file is embedded at compile time via include_str!.
fn titlebar_script() -> String {
    #[cfg(debug_assertions)]
    return r#"(function() {
        fetch('http://localhost:1420/titlebar.js')
            .then(function(r) { return r.text(); })
            .then(function(code) { (0, eval)(code); });
    })();"#.to_string();

    #[cfg(not(debug_assertions))]
    include_str!("../dist/titlebar.js").to_string()
}

fn wait_for_server(timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(stream) = std::net::TcpStream::connect(format!("127.0.0.1:{MARIMO_PORT}")) {
            drop(stream);
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    false
}

fn spawn_marimo() -> std::io::Result<Child> {
    let mut cmd = Command::new("uv");
    cmd.args(["run", "marimo", "edit", "--watch", "--headless", "--no-token", "-p", &MARIMO_PORT.to_string()]);
    #[cfg(unix)]
    cmd.process_group(0);
    cmd.spawn()
}

fn kill_marimo(child: &mut Child) {
    let pgid = child.id();
    let _ = child.kill();
    #[cfg(unix)]
    let _ = Command::new("kill").args(["-9", &format!("-{pgid}")]).status();
}

fn build_marimo_window(
    app: &tauri::AppHandle,
    label: &str,
    url: WebviewUrl,
    features: Option<NewWindowFeatures>,
) -> tauri::Result<tauri::WebviewWindow> {
    let is_main = label == "main";
    let app_for_popup = app.clone();

    let mut builder = WebviewWindowBuilder::new(app, label, url)
        .title("Marimo")
        .inner_size(1280.0, 800.0)
        .decorations(false)
        .on_new_window(move |new_url, new_features| {
            // Plain link clicks are intercepted in the title-bar JS and
            // navigate in-place, so anything reaching here is a user-
            // initiated new window (ctrl/cmd/middle-click, "open in new
            // window", or window.open from a popup window).
            let n = WINDOW_COUNTER.fetch_add(1, Ordering::Relaxed);
            let lbl = format!("marimo-{n}");
            match build_marimo_window(
                &app_for_popup,
                &lbl,
                WebviewUrl::External(new_url),
                Some(new_features),
            ) {
                Ok(window) => NewWindowResponse::Create { window },
                Err(e) => {
                    eprintln!("failed to open popup window: {e}");
                    NewWindowResponse::Deny
                }
            }
        });

    if is_main {
        // No native decorations — our injected title bar handles dragging and controls
        builder = builder
            .initialization_script(&titlebar_script());
    }

    if let Some(f) = features {
        builder = builder.window_features(f).focused(true);
    }

    builder.build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(MarimoProcess(Mutex::new(None)))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            let child = spawn_marimo().expect("failed to spawn marimo via uv");
            *app.state::<MarimoProcess>().0.lock().unwrap() = Some(child);

            let window = build_marimo_window(
                app.handle(),
                "main",
                WebviewUrl::App("index.html".into()),
                None,
            )?;

            std::thread::spawn(move || {
                if wait_for_server(STARTUP_TIMEOUT) {
                    std::thread::sleep(Duration::from_millis(500));
                    window.eval(&format!("window.location.href = '{MARIMO_URL}'")).ok();
                } else {
                    eprintln!("marimo server did not start within timeout");
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    if window.app_handle().webview_windows().len() == 1 {
                        api.prevent_close();
                        let win = window.clone();
                        window.app_handle()
                            .dialog()
                            .message("Closing this window will stop the Marimo server.")
                            .title("Stop Marimo?")
                            .buttons(MessageDialogButtons::OkCancelCustom(
                                "Stop Marimo".into(),
                                "Cancel".into(),
                            ))
                            .show(move |confirmed| {
                                if confirmed {
                                    let state = win.app_handle().state::<MarimoProcess>();
                                    let child = state.0.lock().unwrap().take();
                                    if let Some(mut c) = child {
                                        kill_marimo(&mut c);
                                    }
                                    let _ = win.destroy();
                                }
                            });
                    }
                }
                tauri::WindowEvent::Destroyed => {
                    if window.app_handle().webview_windows().is_empty() {
                        let state = window.app_handle().state::<MarimoProcess>();
                        let child = state.0.lock().unwrap().take();
                        if let Some(mut c) = child {
                            kill_marimo(&mut c);
                        }
                    }
                }
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

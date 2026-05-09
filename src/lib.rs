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

const MARIMO_PORT: u16 = 2730;
const MARIMO_URL: &str = "http://localhost:2730";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

static WINDOW_COUNTER: AtomicUsize = AtomicUsize::new(2);

struct MarimoProcess(Mutex<Option<Child>>);

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
    cmd.args(["run", "marimo", "edit", "--headless", "--no-token", "-p", &MARIMO_PORT.to_string()]);
    // Put the child in its own process group so we can kill the whole tree
    #[cfg(unix)]
    cmd.process_group(0);
    cmd.spawn()
}

fn kill_marimo(child: &mut Child) {
    let pgid = child.id();
    let _ = child.kill();
    // Kill every process in the group (catches marimo spawned by uv)
    #[cfg(unix)]
    let _ = Command::new("kill").args(["-9", &format!("-{pgid}")]).status();
}

fn build_marimo_window(
    app: &tauri::AppHandle,
    label: &str,
    url: WebviewUrl,
    features: Option<NewWindowFeatures>,
) -> tauri::Result<tauri::WebviewWindow> {
    let app_for_popup = app.clone();
    let mut builder = WebviewWindowBuilder::new(app, label, url)
        .title("Marimo")
        .inner_size(1280.0, 800.0)
        .on_new_window(move |new_url, new_features| {
            let n = WINDOW_COUNTER.fetch_add(1, Ordering::Relaxed);
            let label = format!("marimo-{n}");
            match build_marimo_window(
                &app_for_popup,
                &label,
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

    if let Some(f) = features {
        builder = builder.window_features(f).focused(true);
    }

    builder.build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(MarimoProcess(Mutex::new(None)))
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
            if let tauri::WindowEvent::Destroyed = event {
                // Kill marimo when the last window is closed (Destroyed fires
                // after the window is removed from the window list).
                if window.app_handle().webview_windows().is_empty() {
                    let state = window.app_handle().state::<MarimoProcess>();
                    let child = state.0.lock().unwrap().take();
                    if let Some(mut c) = child {
                        kill_marimo(&mut c);
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

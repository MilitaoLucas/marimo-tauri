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
fn titlebar_script() -> String {
    format!(r#"
(function() {{
    var HOME = '{MARIMO_URL}/';
    function onHomePage() {{
        var h = window.location.href;
        return h === HOME || h === HOME.slice(0,-1) || h === '{MARIMO_URL}';
    }}

    // Push marimo's UI below the title bar. Marimo's root uses
    // position:fixed, so margins/padding don't move it. Apply a transform
    // to <body> so it becomes the containing block for fixed descendants,
    // and inset/size body to leave 36px at the top.
    var st = document.createElement('style');
    st.textContent = ''
        + 'html,body{{margin:0!important;padding:0!important}}'
        + 'body{{position:absolute!important;top:36px!important;left:0!important;right:0!important;bottom:0!important;'
        +       'height:calc(100vh - 36px)!important;width:100vw!important;overflow:auto!important;'
        +       'transform:translateZ(0)!important}}';
    (document.head || document.documentElement).appendChild(st);

    // Marimo opens notebooks via <a target="_blank">, which Tauri routes
    // to on_new_window. We want plain clicks to navigate in-place (so the
    // user stays in the main window), and only modifier/middle clicks to
    // create a new window. Intercept plain left-clicks here; let modified
    // clicks fall through to Tauri's on_new_window handler.
    document.addEventListener('click', function(e) {{
        if (e.button !== 0) return;
        if (e.ctrlKey || e.metaKey || e.shiftKey || e.altKey) return;
        if (e.defaultPrevented) return;
        var a = e.target.closest && e.target.closest('a[href]');
        if (!a ) return;
        e.preventDefault();
        window.location.href = a.href;
    }}, true);

    function buildBar() {{
        if (document.getElementById('__tb__')) return;
        var isDark = window.matchMedia('(prefers-color-scheme:dark)').matches;
        var bg   = isDark ? '#1e1e1e' : '#f3f3f3';
        var fg   = isDark ? '#cccccc' : '#333333';
        var sep  = isDark ? 'rgba(255,255,255,0.08)' : 'rgba(0,0,0,0.1)';

        var bar = document.createElement('div');
        bar.id  = '__tb__';
        bar.setAttribute('data-tauri-drag-region','');
        bar.style.cssText = [
            'position:fixed','top:0','left:0','right:0','height:36px',
            'z-index:2147483647','display:flex','align-items:center',
            'background:'+bg,'border-bottom:1px solid '+sep,
            'user-select:none','-webkit-user-select:none',
        ].join(';');

        // ← Home button (hidden on home page)
        if (!onHomePage()) {{
            var home = document.createElement('button');
            home.textContent = '← Home';
            home.style.cssText = [
                'margin-left:8px','padding:3px 10px','border-radius:5px',
                'border:none','background:transparent','cursor:pointer',
                'font-size:12px','font-family:system-ui,sans-serif',
                'color:'+fg,'pointer-events:all','flex-shrink:0',
            ].join(';');
            home.onmouseover = function(){{ this.style.background=isDark?'rgba(255,255,255,0.1)':'rgba(0,0,0,0.07)'; }};
            home.onmouseout  = function(){{ this.style.background='transparent'; }};
            home.onclick = function(e){{ e.stopPropagation(); window.location.href=HOME; }};
            bar.appendChild(home);
        }}

        // Drag spacer
        var drag = document.createElement('div');
        drag.setAttribute('data-tauri-drag-region','');
        drag.style.cssText = 'flex:1;height:100%;';
        bar.appendChild(drag);

        // Window controls
        [
            {{ sym:'−', tip:'Minimize', fn:function(){{ window.__TAURI__.window.getCurrentWindow().minimize(); }} }},
            {{ sym:'□', tip:'Maximize', fn:function(){{ window.__TAURI__.window.getCurrentWindow().toggleMaximize(); }} }},
            {{ sym:'×', tip:'Close',    fn:function(){{ window.__TAURI__.window.getCurrentWindow().close(); }} }},
        ].forEach(function(c) {{
            var b = document.createElement('button');
            b.textContent = c.sym;
            b.title = c.tip;
            b.style.cssText = [
                'width:46px','height:36px','border:none','background:transparent',
                'cursor:pointer','font-size:14px','display:flex','align-items:center',
                'justify-content:center','pointer-events:all','color:'+fg,'flex-shrink:0',
            ].join(';');
            var isClose = c.tip === 'Close';
            b.onmouseover = function(){{ this.style.background = isClose ? '#c42b1c' : (isDark?'rgba(255,255,255,0.1)':'rgba(0,0,0,0.07)'); if(isClose) this.style.color='#fff'; }};
            b.onmouseout  = function(){{ this.style.background='transparent'; this.style.color=fg; }};
            b.onclick = function(e){{ e.stopPropagation(); c.fn(); }};
            bar.appendChild(b);
        }});

        // Attach to <html> so the bar is NOT inside the transformed body
        // (otherwise the transform would also offset the bar itself).
        document.documentElement.appendChild(bar);
    }}

    // documentElement always exists when the init script runs, so we can
    // build the bar synchronously — this avoids a flicker on navigation
    // where the new page would otherwise paint once before DOMContentLoaded.
    buildBar();
}})();
"#)
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

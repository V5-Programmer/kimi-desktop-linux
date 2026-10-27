use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_shell::ShellExt;

/// Same UA as tauri.conf.json. Popups must use it too, otherwise Google
/// rejects the sign-in page as an "embedded browser".
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";

/// Sites that may open as an in-app popup (needed for the Google OAuth flow,
/// which talks back to the opener window via postMessage).
const POPUP_HOSTS: &[&str] = &[
    "kimi.ai",
    "kimi.com",
    "moonshot.cn",
    "google.com",
    "gstatic.com",
];

/// WebKitGTK doesn't close script-opened popups when the page calls
/// `window.close()`. We override it to set this title instead, and close the
/// window from Rust when we see it.
const CLOSE_MARKER: &str = "__kimi_popup_close__";

const POPUP_INIT_SCRIPT: &str = r#"
(function () {
    var nativeClose = window.close.bind(window);
    window.close = function () {
        try { document.title = '__kimi_popup_close__'; } catch (e) {}
        try { nativeClose(); } catch (e) {}
    };
})();
"#;

/// Google's One Tap sign-in prompt renders quite large inside this webview.
/// Scale its container (which lives in Kimi's DOM) down, anchored to the
/// top-right corner where Google places it.
const ONE_TAP_SCALE_SCRIPT: &str = r#"
(function () {
    function addStyle() {
        if (document.getElementById('__kimi_onetap_css')) return;
        var s = document.createElement('style');
        s.id = '__kimi_onetap_css';
        s.textContent = '#credential_picker_container{transform:scale(1);transform-origin:top right;}';
        (document.head || document.documentElement).appendChild(s);
    }
    addStyle();
    document.addEventListener('DOMContentLoaded', addStyle);
})();
"#;

static POPUP_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn is_kimi_host(host: &str) -> bool {
    ["kimi.ai", "kimi.com", "moonshot.cn"]
        .iter()
        .any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

fn is_google_host(host: &str) -> bool {
    host == "google.com" || host.ends_with(".google.com")
}

fn is_popup_host(host: &str) -> bool {
    POPUP_HOSTS
        .iter()
        .any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

/// Called by the Kimi web app (e.g. for SSO / external links) to open a URL in
/// the user's default browser.
#[tauri::command]
fn open_external(app: tauri::AppHandle, url: String) -> Result<(), String> {
    let lower = url.to_lowercase();
    // Only allow safe schemes so a web page can't launch arbitrary handlers.
    if !(lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("mailto:"))
    {
        return Err(format!("blocked url scheme: {url}"));
    }
    #[allow(deprecated)]
    app.shell().open(url, None).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .invoke_handler(tauri::generate_handler![open_external])
        .setup(|app| {
            let handle = app.handle().clone();

            // The main window is declared in tauri.conf.json with
            // `"create": false` so we can build it here and attach a handler
            // for window.open() / target=_blank requests.
            let cfg = app
                .config()
                .app
                .windows
                .iter()
                .find(|w| w.label == "main")
                .cloned()
                .expect("main window config missing from tauri.conf.json");

            let window = WebviewWindowBuilder::from_config(app.handle(), &cfg)?
                .initialization_script(ONE_TAP_SCALE_SCRIPT)
                .on_new_window(move |url, features| {
                    // Logged so we can see what kind of login flow Kimi uses.
                    eprintln!("[kimi-desktop-linux] popup requested: {url}");

                    let is_web = matches!(url.scheme(), "http" | "https");
                    let host = url.host_str().unwrap_or("");

                    // Unrelated websites go to the system browser.
                    if is_web && !is_popup_host(host) {
                        #[allow(deprecated)]
                        let _ = handle.shell().open(url.to_string(), None);
                        return NewWindowResponse::Deny;
                    }

                    // Google sign-in etc.: open an in-app popup that stays
                    // connected to the opener so the login result gets back
                    // to Kimi.
                    let n = POPUP_COUNTER.fetch_add(1, Ordering::Relaxed);
                    // Set once the popup has shown a Google page, so we only
                    // auto-close when it comes *back* to Kimi (login finished).
                    let visited_google = Arc::new(AtomicBool::new(false));
                    let initial_url = url.to_string();
                    let closing = Arc::new(AtomicBool::new(false));
                    let app = handle.clone();
                    let built = WebviewWindowBuilder::new(
                        &handle,
                        format!("popup-{n}"),
                        WebviewUrl::External("about:blank".parse().unwrap()),
                    )
                    .window_features(features)
                    .user_agent(USER_AGENT)
                    .title("Sign in")
                    .initialization_script(POPUP_INIT_SCRIPT)
                    .on_page_load(move |window, payload| {
                        if payload.event() != PageLoadEvent::Finished {
                            return;
                        }
                        let page_url = payload.url().to_string();
                        eprintln!("[kimi-desktop-linux] popup loaded: {page_url}");
                        let host = payload.url().host_str().unwrap_or("").to_string();
                        if is_google_host(&host) {
                            visited_google.store(true, Ordering::Relaxed);
                        } else if is_kimi_host(&host)
                            && (visited_google.load(Ordering::Relaxed) || page_url != initial_url)
                        {
                            // Back on Kimi after Google: the callback page has
                            // run. Give it a moment to hand the result to the
                            // main window, then close the popup.
                            if closing.swap(true, Ordering::Relaxed) {
                                return;
                            }
                            let window = window.clone();
                            let app = app.clone();
                            std::thread::spawn(move || {
                                std::thread::sleep(Duration::from_millis(3000));
                                let _ = window.close();
                                // The session cookie is shared with the main
                                // window, but its page doesn't know yet. Send it
                                // back to the home page so it picks up the login.
                                std::thread::sleep(Duration::from_millis(800));
                                if let Some(main) = app.get_webview_window("main") {
                                    let _ = main.eval(
                                        "window.location.href = window.location.origin + '/'",
                                    );
                                }
                            });
                        }
                    })
                    .on_document_title_changed(|window, title| {
                        if title == CLOSE_MARKER {
                            let _ = window.close();
                        } else {
                            let _ = window.set_title(&title);
                        }
                    })
                    .build();

                    match built {
                        Ok(window) => {
                            // Compact, Google-sized popup instead of the large default.
                            let _ = window.set_size(tauri::LogicalSize::new(500.0, 640.0));
                            let _ = window.center();
                            NewWindowResponse::Create { window }
                        }
                        Err(e) => {
                            eprintln!("[kimi-desktop-linux] failed to open popup: {e}");
                            NewWindowResponse::Deny
                        }
                    }
                })
                .build()?;

            #[cfg(debug_assertions)]
            {
                window.open_devtools();
            }
            let _ = window.eval(PHONE_CODE_AUTOFILL_SCRIPT);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run app");
}

/// Kimi's web app requires the verification code field to be non-empty before
/// the Send button triggers SMS delivery. This helper auto-fills a dummy digit
/// when Send is clicked on an empty field.
const PHONE_CODE_AUTOFILL_SCRIPT: &str = r#"
(function() {
    if (window.__kimiPhoneHelperInstalled) return;
    window.__kimiPhoneHelperInstalled = true;

    function findCodeInput(container) {
        var inputs = container.querySelectorAll('input[type="text"], input[type="number"], input[type="tel"], input:not([type])');
        for (var i = 0; i < inputs.length; i++) {
            var input = inputs[i];
            var placeholder = (input.placeholder || '').toLowerCase();
            var name = (input.name || '').toLowerCase();
            var id = (input.id || '').toLowerCase();
            if (placeholder.indexOf('verification') !== -1 || placeholder.indexOf('code') !== -1 ||
                name.indexOf('code') !== -1 || id.indexOf('code') !== -1) {
                return input;
            }
        }
        return null;
    }

    document.addEventListener('click', function(e) {
        var btn = e.target.closest('button, [role="button"]');
        if (!btn) return;
        var text = (btn.textContent || btn.innerText || '').trim().toLowerCase();
        if (text !== 'send' && text !== '发送' && text !== '获取验证码') return;

        var container = btn.closest('form, div, section') || document.body;
        var codeInput = findCodeInput(container);
        if (codeInput && !codeInput.value.trim()) {
            codeInput.value = '0';
            codeInput.dispatchEvent(new Event('input', { bubbles: true }));
            codeInput.dispatchEvent(new Event('change', { bubbles: true }));
            console.log('[kimi-desktop-linux] Auto-filled dummy code for Send button');
        }
    }, true);
})();
"#;

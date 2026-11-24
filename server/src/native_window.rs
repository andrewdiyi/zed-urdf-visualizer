#[cfg(any(target_os = "macos", target_os = "windows"))]
mod desktop {
    use std::sync::Arc;
    use tao::dpi::{LogicalPosition, LogicalSize};
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoopBuilder};
    use tao::window::WindowBuilder;
    use wry::WebViewBuilder;

    enum UserCmd {
        SetAlwaysOnTop(bool),
        OpenBrowser(String),
    }

    pub fn run_native_window(url: &str, pin_on_top: bool) -> anyhow::Result<()> {
        let event_loop = EventLoopBuilder::<UserCmd>::with_user_event().build();
        let proxy = event_loop.create_proxy();

        let mut wb = WindowBuilder::new()
            .with_title("Zed URDF Visualizer")
            .with_inner_size(LogicalSize::new(980.0, 700.0))
            .with_min_inner_size(LogicalSize::new(520.0, 380.0))
            .with_always_on_top(pin_on_top);

        if let Some(monitor) = event_loop.primary_monitor() {
            let scale = monitor.scale_factor();
            let logical = monitor.size().to_logical::<f64>(scale);
            let x = (logical.width - 1000.0).max(40.0);
            let y = ((logical.height - 720.0) * 0.5).max(40.0);
            wb = wb.with_position(LogicalPosition::new(x, y));
        }

        let window = Arc::new(wb.build(&event_loop)?);
        let window_for_ipc = window.clone();

        let init_script = if pin_on_top {
            r#"
            window.addEventListener('DOMContentLoaded', () => {
              const pinBtn = document.getElementById('btn-pin-top');
              if (pinBtn) pinBtn.setAttribute('aria-pressed', 'true');
            });
            "#
        } else {
            ""
        };

        let _webview = WebViewBuilder::new()
            .with_url(url)
            .with_initialization_script(init_script)
            .with_ipc_handler(move |req| {
                let body = req.body();
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(body) {
                    match val.get("cmd").and_then(|c| c.as_str()) {
                        Some("drag_window") => {
                            let _ = window_for_ipc.drag_window();
                        }
                        Some("set_always_on_top") => {
                            let pinned = val
                                .get("value")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            let _ = proxy.send_event(UserCmd::SetAlwaysOnTop(pinned));
                        }
                        Some("open_browser") => {
                            if let Some(u) = val.get("url").and_then(|v| v.as_str()) {
                                let _ = proxy.send_event(UserCmd::OpenBrowser(u.to_string()));
                            }
                        }
                        _ => {}
                    }
                }
            })
            .build(&*window)?;

        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;

            match event {
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    *control_flow = ControlFlow::Exit;
                }
                Event::UserEvent(cmd) => match cmd {
                    UserCmd::SetAlwaysOnTop(val) => {
                        window.set_always_on_top(val);
                    }
                    UserCmd::OpenBrowser(target_url) => {
                        let _ = open::that(target_url);
                    }
                },
                _ => {}
            }
        });
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use desktop::run_native_window;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn run_native_window(url: &str, _pin_on_top: bool) -> anyhow::Result<()> {
    let clean_url = url.replace("?native=1", "");
    if std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some() {
        open::that(&clean_url)?;
    } else {
        eprintln!("Headless Linux environment detected. Open {clean_url} via SSH port forwarding.");
    }
    Ok(())
}

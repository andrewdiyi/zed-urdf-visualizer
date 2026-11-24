mod analyzer;
mod http_server;
mod lsp_server;
mod native_window;
mod packages;
mod state;
mod xacro;

use clap::{Parser, Subcommand};
use http_server::start_http_server;
use lsp_server::UrdfLanguageServer;
use state::{session_file_path, SessionInfo, SharedState};
use std::path::PathBuf;
use tower_lsp::{LspService, Server};

#[derive(Parser, Debug)]
#[command(name = "zed-urdf-server", version, about = "Zed URDF Visualizer LSP & 3D Companion Server")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run as Language Server (stdio) + embedded HTTP/WS 3D preview server
    Lsp {
        #[arg(long, default_value_t = 0)]
        port: u16,
    },
    /// Open native frameless/overlay floating WebView window
    Window {
        #[arg(long)]
        url: String,
        #[arg(long, default_value_t = false)]
        pin: bool,
    },
    /// Standalone preview mode for a URDF or Xacro file
    Serve {
        /// Path to .urdf or .xacro file
        file: PathBuf,
        #[arg(long, default_value_t = 0)]
        port: u16,
        /// Open in system browser instead of native floating window
        #[arg(long, default_value_t = false)]
        browser: bool,
        /// Headless HTTP server only (do not open window automatically)
        #[arg(long, default_value_t = false)]
        headless: bool,
        /// Extra package mappings: pkg_name=/path/to/pkg
        #[arg(long = "pkg")]
        packages: Vec<String>,
    },
    /// Trigger preview on an active Zed LSP session (or fall back to standalone Serve)
    OpenPreview {
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        browser: bool,
    },
    /// Establish an SSH port-forwarding tunnel to a remote Zed SSH host and open local 3D preview
    Ssh {
        /// SSH target (e.g. `user@your-server` or SSH config alias)
        target: String,
        /// SSH port on remote server
        #[arg(short = 'p', long, default_value_t = 22)]
        ssh_port: u16,
        /// Remote URDF server HTTP port (defaults to 19876)
        #[arg(long, default_value_t = 19876)]
        remote_port: u16,
        /// Local port to bind on this machine (defaults to 19876)
        #[arg(long, default_value_t = 19876)]
        local_port: u16,
        /// Open in system browser instead of native floating window
        #[arg(long, default_value_t = false)]
        browser: bool,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command.unwrap_or(Commands::Lsp { port: 0 }) {
        Commands::Window { url, pin } => {
            // Must run on main thread for macOS Cocoa EventLoop!
            native_window::run_native_window(&url, pin)
        }
        Commands::Lsp { port } => run_tokio(async move {
            let state = SharedState::new();
            let stdin = tokio::io::stdin();
            let stdout = tokio::io::stdout();
            let (service, socket) =
                LspService::new(move |client| UrdfLanguageServer::new(client, state.clone(), port));
            Server::new(stdin, stdout, socket).serve(service).await;
            Ok(())
        }),
        Commands::Serve {
            file,
            port,
            browser,
            headless,
            packages,
        } => run_tokio(async move {
            let abs_file = std::fs::canonicalize(&file).unwrap_or(file);
            let state = SharedState::new();

            if !packages.is_empty() {
                let mut map = std::collections::BTreeMap::new();
                for entry in packages {
                    if let Some((k, v)) = entry.split_once('=') {
                        map.insert(k.to_string(), v.to_string());
                    }
                }
                state.registry.write().await.apply_user_overrides(map, None);
            }

            let bound_port = start_http_server(state.clone(), port).await?;
            let content = std::fs::read_to_string(&abs_file)?;
            state.update_document(abs_file.clone(), content).await;

            eprintln!("URDF Visualizer Server running at http://127.0.0.1:{bound_port}/");

            if !headless {
                if browser {
                    state.open_in_browser().await?;
                } else {
                    state.spawn_native_window().await?;
                }
            }

            // Watch file modifications on disk when running in standalone `serve` mode
            let mut last_mtime = std::fs::metadata(&abs_file).and_then(|m| m.modified()).ok();
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(350)).await;
                let curr_mtime = std::fs::metadata(&abs_file).and_then(|m| m.modified()).ok();
                if curr_mtime.is_some() && curr_mtime != last_mtime {
                    last_mtime = curr_mtime;
                    if let Ok(updated) = std::fs::read_to_string(&abs_file) {
                        state.update_document(abs_file.clone(), updated).await;
                    }
                }
            }
        }),
        Commands::OpenPreview { file, browser } => run_tokio(async move {
            if let Ok(raw) = std::fs::read_to_string(session_file_path()) {
                if let Ok(info) = serde_json::from_str::<SessionInfo>(&raw) {
                    if send_open_preview_rpc(info.port, file.as_ref(), browser)
                        .await
                        .is_ok()
                    {
                        return Ok(());
                    }
                }
            }
            if let Some(f) = file {
                let abs_file = std::fs::canonicalize(&f).unwrap_or(f);
                let state = SharedState::new();
                start_http_server(state.clone(), 0).await?;
                let content = std::fs::read_to_string(&abs_file)?;
                state.update_document(abs_file.clone(), content).await;
                if browser {
                    state.open_in_browser().await?;
                    tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                } else {
                    state.spawn_native_window().await?;
                    // Wait until native window exits
                    loop {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        let exited = {
                            let mut guard = state.viewer_child.lock().unwrap();
                            match guard.as_mut() {
                                Some(child) => child.try_wait().ok().flatten().is_some(),
                                None => true,
                            }
                        };
                        if exited {
                            break;
                        }
                    }
                }
            }
            Ok(())
        }),
        Commands::Ssh {
            target,
            ssh_port,
            remote_port,
            local_port,
            browser,
        } => {
            let mut ssh_child = std::process::Command::new("ssh")
                .arg("-N")
                .arg("-o")
                .arg("ExitOnForwardFailure=yes")
                .arg("-L")
                .arg(format!("{local_port}:127.0.0.1:{remote_port}"))
                .arg("-p")
                .arg(ssh_port.to_string())
                .arg(&target)
                .spawn()?;

            std::thread::sleep(std::time::Duration::from_millis(450));

            if browser {
                let url = format!("http://127.0.0.1:{local_port}/");
                eprintln!("SSH tunnel active → {url} (Press Ctrl+C to close)");
                open::that(&url)?;
                let _ = ssh_child.wait();
                Ok(())
            } else {
                let url = format!("http://127.0.0.1:{local_port}/?native=1");
                // Clean up SSH tunnel automatically if window closes on platforms where run_native_window returns
                struct KillOnDrop(std::process::Child);
                impl Drop for KillOnDrop {
                    fn drop(&mut self) {
                        let _ = self.0.kill();
                    }
                }
                let _guard = KillOnDrop(ssh_child);
                native_window::run_native_window(&url, true)
            }
        }
    }
}

fn run_tokio<F>(fut: F) -> anyhow::Result<()>
where
    F: std::future::Future<Output = anyhow::Result<()>>,
{
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(fut)
}

async fn send_open_preview_rpc(
    port: u16,
    file: Option<&PathBuf>,
    browser: bool,
) -> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
    let body = serde_json::json!({
        "file_path": file.map(|p| p.to_string_lossy().to_string()),
        "browser": browser
    })
    .to_string();
    let req = format!(
        "POST /api/open-preview HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(req.as_bytes()).await?;
    let mut resp = String::new();
    stream.read_to_string(&mut resp).await?;
    if resp.starts_with("HTTP/1.1 200") || resp.starts_with("HTTP/1.0 200") {
        Ok(())
    } else {
        anyhow::bail!("Active session returned non-200")
    }
}

use crate::analyzer::{analyze_urdf_or_xacro, AnalysisReport};
use crate::packages::PackageRegistry;
use crate::xacro::{is_xacro_document, process_xacro};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Child;
#[cfg(not(target_os = "linux"))]
use std::process::{Command, Stdio};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelUpdatePayload {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub file_path: String,
    pub file_name: String,
    pub working_dir: String,
    pub urdf_xml: Option<String>,
    pub error: Option<String>,
    pub is_xacro: bool,
    pub xacro_args: BTreeMap<String, String>,
    pub packages: BTreeMap<String, String>,
    pub diagnostics: Vec<String>,
    pub language: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ExtensionSettings {
    #[serde(default)]
    pub packages: BTreeMap<String, String>,
    #[serde(default)]
    pub xacro_args: BTreeMap<String, String>,
    #[serde(default)]
    pub xacro_command: Option<String>,
    #[serde(default = "default_auto_open")]
    pub auto_open_viewer: bool,
    #[serde(default)]
    pub prefer_browser: bool,
    #[serde(default = "default_pin_on_top")]
    pub always_on_top: bool,
    #[serde(default = "default_remote_port")]
    pub remote_port: u16,
    #[serde(default)]
    pub remote_allow_lan: bool,
    #[serde(default = "default_language")]
    pub language: String,
}

fn default_language() -> String {
    "en".to_string()
}

fn default_auto_open() -> bool {
    false
}

fn default_pin_on_top() -> bool {
    true
}

pub fn default_remote_port() -> u16 {
    19876
}

pub fn is_ssh_session() -> bool {
    std::env::var_os("SSH_CONNECTION").is_some()
        || std::env::var_os("SSH_CLIENT").is_some()
        || std::env::var_os("SSH_TTY").is_some()
}

#[derive(Debug, Clone)]
pub struct OpenDoc {
    pub text: String,
    pub analysis: AnalysisReport,
}

pub struct SharedState {
    pub http_port: RwLock<u16>,
    pub workspace_root: RwLock<Option<PathBuf>>,
    pub settings: RwLock<ExtensionSettings>,
    pub registry: RwLock<PackageRegistry>,
    pub docs: RwLock<HashMap<PathBuf, OpenDoc>>,
    pub active_file: RwLock<Option<PathBuf>>,
    pub latest_payload: RwLock<Option<ModelUpdatePayload>>,
    pub ws_tx: broadcast::Sender<String>,
    pub viewer_child: std::sync::Mutex<Option<Child>>,
    pub ssh_notified: std::sync::atomic::AtomicBool,
}

impl SharedState {
    pub fn new() -> Arc<Self> {
        let (ws_tx, _) = broadcast::channel(64);
        Arc::new(Self {
            http_port: RwLock::new(0),
            workspace_root: RwLock::new(None),
            settings: RwLock::new(ExtensionSettings {
                auto_open_viewer: false,
                always_on_top: true,
                remote_port: default_remote_port(),
                remote_allow_lan: false,
                language: default_language(),
                ..Default::default()
            }),
            registry: RwLock::new(PackageRegistry::new()),
            docs: RwLock::new(HashMap::new()),
            active_file: RwLock::new(None),
            latest_payload: RwLock::new(None),
            ws_tx,
            viewer_child: std::sync::Mutex::new(None),
            ssh_notified: std::sync::atomic::AtomicBool::new(false),
        })
    }

    pub async fn ssh_forward_instructions(&self) -> Option<String> {
        if !is_ssh_session() {
            return None;
        }
        let port = *self.http_port.read().await;
        Some(format!(
            "Remote URDF preview ready at http://127.0.0.1:{port} (configure `LocalForward {port} 127.0.0.1:{port}` in ~/.ssh/config or run `zed-urdf-server ssh <host>`)"
        ))
    }

    pub async fn viewer_url(&self, native: bool) -> String {
        let port = *self.http_port.read().await;
        if native {
            format!("http://127.0.0.1:{port}/?native=1")
        } else {
            format!("http://127.0.0.1:{port}/")
        }
    }

    pub async fn spawn_native_window(&self) -> anyhow::Result<()> {
        if is_ssh_session() {
            return Ok(());
        }
        #[cfg(target_os = "linux")]
        {
            return self.open_in_browser().await;
        }
        #[cfg(not(target_os = "linux"))]
        {
            // Check if an existing native viewer window is still alive
            {
                let mut guard = self.viewer_child.lock().unwrap();
                if let Some(child) = guard.as_mut() {
                    if let Ok(None) = child.try_wait() {
                        return Ok(());
                    }
                }
                *guard = None;
            }

            let url = self.viewer_url(true).await;
            let pin = self.settings.read().await.always_on_top;
            let exe = std::env::current_exe()?;
            let mut cmd = Command::new(exe);
            cmd.arg("window").arg("--url").arg(&url);
            if pin {
                cmd.arg("--pin");
            }
            cmd.stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());

            let child = cmd.spawn()?;
            *self.viewer_child.lock().unwrap() = Some(child);
            Ok(())
        }
    }

    pub async fn open_in_browser(&self) -> anyhow::Result<()> {
        let url = self.viewer_url(false).await;
        open::that(url)?;
        Ok(())
    }

    pub async fn update_document(
        &self,
        path: PathBuf,
        text: String,
    ) -> AnalysisReport {
        {
            let mut reg = self.registry.write().await;
            reg.register_file_context(&path);
        }

        let settings = self.settings.read().await.clone();
        let registry_snapshot = self.registry.read().await.clone();

        let is_xacro = is_xacro_document(&path, &text);
        let mut urdf_xml: Option<String> = None;
        let mut xacro_err: Option<String> = None;
        let mut discovered_args = settings.xacro_args.clone();

        if is_xacro {
            match process_xacro(
                &text,
                &path,
                &registry_snapshot,
                &settings.xacro_args,
                settings.xacro_command.as_deref(),
            ) {
                Ok(res) => {
                    urdf_xml = Some(res.urdf_xml);
                    discovered_args = res.discovered_args;
                }
                Err(e) => {
                    xacro_err = Some(e.to_string());
                }
            }
        } else {
            urdf_xml = Some(text.clone());
        }

        let analysis = analyze_urdf_or_xacro(
            &text,
            if is_xacro { urdf_xml.as_deref() } else { None },
            &path,
            &registry_snapshot,
        );

        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("model.urdf")
            .to_string();
        let working_dir = path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_string_lossy()
            .to_string();

        // Keep previous valid URDF if user is mid-keystroke with temporary XML syntax error
        let fallback_urdf = if urdf_xml.is_none() || roxmltree::Document::parse(urdf_xml.as_deref().unwrap_or("")).is_err() {
            self.latest_payload
                .read()
                .await
                .as_ref()
                .filter(|prev| prev.file_path == path.to_string_lossy())
                .and_then(|prev| prev.urdf_xml.clone())
                .or(urdf_xml)
        } else {
            urdf_xml
        };

        let combined_err = xacro_err.or_else(|| {
            analysis
                .diagnostics
                .iter()
                .find(|d| d.severity == Some(tower_lsp::lsp_types::DiagnosticSeverity::ERROR))
                .map(|d| d.message.clone())
        });

        let payload = ModelUpdatePayload {
            msg_type: "model_update".to_string(),
            file_path: path.to_string_lossy().to_string(),
            file_name,
            working_dir,
            urdf_xml: fallback_urdf,
            error: combined_err,
            is_xacro,
            xacro_args: discovered_args,
            packages: registry_snapshot.merged_map(),
            diagnostics: analysis.warning_messages.clone(),
            language: settings.language.clone(),
        };

        if let Ok(json) = serde_json::to_string(&payload) {
            let _ = self.ws_tx.send(json);
        }

        *self.active_file.write().await = Some(path.clone());
        *self.latest_payload.write().await = Some(payload);
        self.docs.write().await.insert(
            path,
            OpenDoc {
                text,
                analysis: analysis.clone(),
            },
        );

        analysis
    }

    pub async fn set_xacro_arg_and_reprocess(&self, key: String, value: String) {
        self.settings.write().await.xacro_args.insert(key, value);
        let active = self.active_file.read().await.clone();
        if let Some(path) = active {
            let maybe_text = self
                .docs
                .read()
                .await
                .get(&path)
                .map(|d| d.text.clone())
                .or_else(|| std::fs::read_to_string(&path).ok());
            if let Some(text) = maybe_text {
                let _ = self.update_document(path, text).await;
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionInfo {
    pub pid: u32,
    pub port: u16,
}

fn user_tag() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "default".to_string())
}

pub fn session_file_path() -> PathBuf {
    std::env::temp_dir().join(format!("zed-urdf-visualizer-{}.json", user_tag()))
}

pub fn port_file_path() -> PathBuf {
    std::env::temp_dir().join(format!("zed-urdf-visualizer-{}.port", user_tag()))
}

pub fn write_session_info(port: u16) {
    let info = SessionInfo {
        pid: std::process::id(),
        port,
    };
    if let Ok(s) = serde_json::to_string(&info) {
        let _ = std::fs::write(session_file_path(), s);
    }
    let _ = std::fs::write(port_file_path(), port.to_string());
}

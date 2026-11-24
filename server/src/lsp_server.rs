use crate::analyzer::{make_markdown_hover, position_in_range};
use crate::http_server::start_http_server;
use crate::state::{is_ssh_session, ExtensionSettings, SharedState};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

pub struct UrdfLanguageServer {
    pub client: Client,
    pub state: Arc<SharedState>,
    pub cli_port: u16,
}

impl UrdfLanguageServer {
    pub fn new(client: Client, state: Arc<SharedState>, cli_port: u16) -> Self {
        Self {
            client,
            state,
            cli_port,
        }
    }

    async fn apply_json_settings(&self, value: &serde_json::Value) {
        let candidate = value.get("urdf-lsp").unwrap_or(value);
        if let Ok(parsed) = serde_json::from_value::<ExtensionSettings>(candidate.clone()) {
            let ws_root = self.state.workspace_root.read().await.clone();
            {
                let mut reg = self.state.registry.write().await;
                reg.apply_user_overrides(parsed.packages.clone(), ws_root.as_deref());
            }
            *self.state.settings.write().await = parsed;
        }
    }

    async fn handle_doc_event(&self, uri: Url, text: String, is_initial_open: bool) {
        let Ok(path) = uri.to_file_path() else {
            return;
        };
        let report = self.state.update_document(path, text).await;
        self.client
            .publish_diagnostics(uri, report.diagnostics, None)
            .await;

        if is_initial_open {
            if let Some(ssh_msg) = self.state.ssh_forward_instructions().await {
                if !self.state.ssh_notified.swap(true, Ordering::SeqCst) {
                    self.client.show_message(MessageType::INFO, ssh_msg).await;
                }
            } else {
                let settings = self.state.settings.read().await.clone();
                if settings.auto_open_viewer {
                    if settings.prefer_browser {
                        let _ = self.state.open_in_browser().await;
                    } else {
                        let _ = self.state.spawn_native_window().await;
                    }
                }
            }
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for UrdfLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let mut roots_to_scan = Vec::new();
        if let Some(folders) = params.workspace_folders {
            for f in folders {
                if let Ok(path) = f.uri.to_file_path() {
                    if self.state.workspace_root.read().await.is_none() {
                        *self.state.workspace_root.write().await = Some(path.clone());
                    }
                    self.state.registry.write().await.allowed_roots.insert(path.clone());
                    roots_to_scan.push(path);
                }
            }
        } else if let Some(root_uri) = params.root_uri {
            if let Ok(path) = root_uri.to_file_path() {
                *self.state.workspace_root.write().await = Some(path.clone());
                self.state.registry.write().await.allowed_roots.insert(path.clone());
                roots_to_scan.push(path);
            }
        }

        if !roots_to_scan.is_empty() {
            let state_bg = self.state.clone();
            tokio::spawn(async move {
                for r in roots_to_scan {
                    state_bg.registry.write().await.scan_directory(&r, 5);
                }
            });
        }

        if let Some(init_opts) = params.initialization_options {
            self.apply_json_settings(&init_opts).await;
        }

        if *self.state.http_port.read().await == 0 {
            let _ = start_http_server(self.state.clone(), self.cli_port).await;
        }

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "zed-urdf-server".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                definition_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec![
                        "urdf.openNativeViewer".into(),
                        "urdf.openBrowserViewer".into(),
                        "urdf.showSshForwardInfo".into(),
                        "urdf.rescanPackages".into(),
                    ],
                    ..Default::default()
                }),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        let port = *self.state.http_port.read().await;
        self.client
            .log_message(
                MessageType::INFO,
                format!("URDF Visualizer LSP active on http://127.0.0.1:{port}"),
            )
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        if let Some(mut child) = self.state.viewer_child.lock().unwrap().take() {
            let _ = child.kill();
        }
        Ok(())
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        self.apply_json_settings(&params.settings).await;
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.handle_doc_event(params.text_document.uri, params.text_document.text, true)
            .await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.pop() {
            self.handle_doc_event(params.text_document.uri, change.text, false)
                .await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        if let Some(text) = params.text {
            self.handle_doc_event(params.text_document.uri, text, false)
                .await;
        } else if let Ok(path) = params.text_document.uri.to_file_path() {
            if let Ok(disk_text) = std::fs::read_to_string(&path) {
                self.handle_doc_event(params.text_document.uri, disk_text, false)
                    .await;
            }
        }
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let Ok(path) = uri.to_file_path() else {
            return Ok(None);
        };

        let docs = self.state.docs.read().await;
        let Some(doc) = docs.get(&path) else {
            return Ok(None);
        };

        for (range, target_loc) in &doc.analysis.ref_links {
            if position_in_range(pos, *range) {
                return Ok(Some(GotoDefinitionResponse::Scalar(target_loc.clone())));
            }
        }
        Ok(None)
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let Ok(path) = uri.to_file_path() else {
            return Ok(None);
        };

        let docs = self.state.docs.read().await;
        let Some(doc) = docs.get(&path) else {
            return Ok(None);
        };

        for (range, md) in doc.analysis.hovers.iter().rev() {
            if position_in_range(pos, *range) {
                return Ok(Some(make_markdown_hover(md.clone(), *range)));
            }
        }
        Ok(None)
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let Ok(path) = params.text_document.uri.to_file_path() else {
            return Ok(None);
        };
        let docs = self.state.docs.read().await;
        let Some(doc) = docs.get(&path) else {
            return Ok(None);
        };
        Ok(Some(DocumentSymbolResponse::Nested(
            doc.analysis.symbols.clone(),
        )))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri_str = params.text_document.uri.to_string();
        let mut actions = Vec::new();

        if is_ssh_session() {
            let port = *self.state.http_port.read().await;
            actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: format!("URDF Visualizer: 查看远端 SSH 转发预览地址 (端口 {port})"),
                kind: Some(CodeActionKind::EMPTY),
                command: Some(Command {
                    title: "查看远端 SSH 转发预览地址".into(),
                    command: "urdf.showSshForwardInfo".into(),
                    arguments: Some(vec![serde_json::Value::String(uri_str)]),
                }),
                ..Default::default()
            }));
        } else {
            actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: "URDF Visualizer: 打开原生 3D 悬浮窗预览".into(),
                kind: Some(CodeActionKind::EMPTY),
                command: Some(Command {
                    title: "打开原生 3D 悬浮窗预览".into(),
                    command: "urdf.openNativeViewer".into(),
                    arguments: Some(vec![serde_json::Value::String(uri_str.clone())]),
                }),
                ..Default::default()
            }));
            actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: "URDF Visualizer: 在浏览器中打开 3D 预览".into(),
                kind: Some(CodeActionKind::EMPTY),
                command: Some(Command {
                    title: "在浏览器中打开 3D 预览".into(),
                    command: "urdf.openBrowserViewer".into(),
                    arguments: Some(vec![serde_json::Value::String(uri_str)]),
                }),
                ..Default::default()
            }));
        }

        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
            title: "URDF Visualizer: 重新扫描工作区 ROS Package".into(),
            kind: Some(CodeActionKind::EMPTY),
            command: Some(Command {
                title: "重新扫描 ROS Package".into(),
                command: "urdf.rescanPackages".into(),
                arguments: None,
            }),
            ..Default::default()
        }));

        Ok(Some(actions))
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        if let Some(first_arg) = params.arguments.first().and_then(|v| v.as_str()) {
            if let Ok(uri) = Url::parse(first_arg) {
                if let Ok(path) = uri.to_file_path() {
                    let maybe_text = self
                        .state
                        .docs
                        .read()
                        .await
                        .get(&path)
                        .map(|d| d.text.clone())
                        .or_else(|| std::fs::read_to_string(&path).ok());
                    if let Some(text) = maybe_text {
                        let _ = self.state.update_document(path, text).await;
                    }
                }
            }
        }

        match params.command.as_str() {
            "urdf.openNativeViewer" => {
                if let Err(e) = self.state.spawn_native_window().await {
                    self.client
                        .show_message(MessageType::ERROR, format!("打开悬浮预览窗失败: {e}"))
                        .await;
                }
            }
            "urdf.openBrowserViewer" => {
                if let Err(e) = self.state.open_in_browser().await {
                    self.client
                        .show_message(MessageType::ERROR, format!("打开浏览器预览失败: {e}"))
                        .await;
                }
            }
            "urdf.showSshForwardInfo" => {
                if let Some(msg) = self.state.ssh_forward_instructions().await {
                    self.client.show_message(MessageType::INFO, msg).await;
                }
            }
            "urdf.rescanPackages" => {
                if let Some(ws) = self.state.workspace_root.read().await.clone() {
                    self.state.registry.write().await.add_workspace_root(&ws);
                }
                self.state.registry.write().await.scan_env_paths();
                let count = self.state.registry.read().await.merged_map().len();
                self.client
                    .show_message(
                        MessageType::INFO,
                        format!("已扫描并索引 {count} 个 ROS Package"),
                    )
                    .await;
            }
            _ => {}
        }

        Ok(None)
    }
}

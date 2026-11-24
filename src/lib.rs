use std::fs;
use zed_extension_api::{
    self as zed,
    serde_json::{self, json},
    settings::LspSettings,
    Architecture, Command, DownloadedFileType, GithubReleaseOptions, LanguageServerId,
    LanguageServerInstallationStatus, Os, Result, Worktree,
};

/// Default GitHub repository hosting prebuilt `zed-urdf-server` release binaries.
/// Can also be overridden via `lsp.urdf-lsp.settings.github_repo` in Zed settings.
const DEFAULT_GITHUB_REPO: &str = "andrewdiyi/zed-urdf-visualizer";

struct UrdfVisualizerExtension {
    cached_binary_path: Option<String>,
}

impl UrdfVisualizerExtension {
    fn platform_asset_info() -> Result<(&'static str, DownloadedFileType, &'static str)> {
        let (os, arch) = zed::current_platform();
        match (os, arch) {
            (Os::Mac, Architecture::Aarch64) => Ok((
                "zed-urdf-server-aarch64-apple-darwin.tar.gz",
                DownloadedFileType::GzipTar,
                "zed-urdf-server",
            )),
            (Os::Mac, Architecture::X8664) => Ok((
                "zed-urdf-server-x86_64-apple-darwin.tar.gz",
                DownloadedFileType::GzipTar,
                "zed-urdf-server",
            )),
            (Os::Linux, Architecture::X8664) => Ok((
                "zed-urdf-server-x86_64-unknown-linux-musl.tar.gz",
                DownloadedFileType::GzipTar,
                "zed-urdf-server",
            )),
            (Os::Linux, Architecture::Aarch64) => Ok((
                "zed-urdf-server-aarch64-unknown-linux-musl.tar.gz",
                DownloadedFileType::GzipTar,
                "zed-urdf-server",
            )),
            (Os::Windows, Architecture::X8664) => Ok((
                "zed-urdf-server-x86_64-pc-windows-msvc.zip",
                DownloadedFileType::Zip,
                "zed-urdf-server.exe",
            )),
            _ => Err(format!("暂不支持当前平台架构: {os:?} / {arch:?}")),
        }
    }

    fn find_cached_work_dir_binary(bin_name: &str) -> Option<String> {
        let entries = fs::read_dir(".").ok()?;
        let mut candidates: Vec<String> = Vec::new();
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name.starts_with("zed-urdf-server-") && entry.path().is_dir() {
                let bin_path = format!("{file_name}/{bin_name}");
                if fs::metadata(&bin_path).is_ok_and(|m| m.is_file()) {
                    candidates.push(bin_path);
                }
            }
        }
        candidates.sort();
        candidates.pop()
    }

    fn resolve_server_binary(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<(String, Vec<String>, Vec<(String, String)>)> {
        let lsp_settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree).ok();
        let shell_env = worktree.shell_env();

        // 1. Explicit binary path configured in Zed settings.json
        if let Some(settings) = &lsp_settings {
            if let Some(bin) = &settings.binary {
                if let Some(path) = &bin.path {
                    let args = bin
                        .arguments
                        .clone()
                        .unwrap_or_else(|| vec!["lsp".to_string()]);
                    let env = bin
                        .env
                        .as_ref()
                        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                        .unwrap_or_else(|| shell_env.clone());
                    return Ok((path.clone(), args, env));
                }
            }
        }

        // 2. Reuse in-memory cached binary path if already verified in this session
        if let Some(cached) = &self.cached_binary_path {
            if fs::metadata(cached).is_ok_and(|m| m.is_file()) {
                return Ok((cached.clone(), vec!["lsp".to_string()], shell_env));
            }
        }

        let (asset_name, file_type, bin_name) = Self::platform_asset_info()?;

        // 3. Read optional custom repo or GitHub download proxy from user settings
        let raw_settings = lsp_settings.as_ref().and_then(|s| s.settings.as_ref());
        let github_repo = raw_settings
            .and_then(|v| v.get("github_repo"))
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_GITHUB_REPO);
        let github_proxy = raw_settings
            .and_then(|v| v.get("github_proxy"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // 4. Check already-downloaded version inside Zed's extension sandbox first
        //    (avoids blocking on GitHub API in offline or restricted SSH environments)
        if let Some(existing_cached) = Self::find_cached_work_dir_binary(bin_name) {
            self.cached_binary_path = Some(existing_cached.clone());
            return Ok((existing_cached, vec!["lsp".to_string()], shell_env));
        }

        // 5. Query GitHub Releases & auto-download into Zed's extension work directory
        zed::set_language_server_installation_status(
            language_server_id,
            &LanguageServerInstallationStatus::CheckingForUpdate,
        );

        match zed::latest_github_release(
            github_repo,
            GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        ) {
            Ok(release) => {
                if let Some(asset) = release.assets.iter().find(|a| a.name == asset_name) {
                    let version_dir = format!("zed-urdf-server-{}", release.version);
                    let binary_path = format!("{version_dir}/{bin_name}");

                    if !fs::metadata(&binary_path).is_ok_and(|m| m.is_file()) {
                        zed::set_language_server_installation_status(
                            language_server_id,
                            &LanguageServerInstallationStatus::Downloading,
                        );

                        let download_url = if github_proxy.is_empty() {
                            asset.download_url.clone()
                        } else {
                            format!(
                                "{}/{}",
                                github_proxy.trim_end_matches('/'),
                                asset.download_url
                            )
                        };

                        if let Err(dl_err) = zed::download_file(&download_url, &version_dir, file_type)
                        {
                            zed::set_language_server_installation_status(
                                language_server_id,
                                &LanguageServerInstallationStatus::None,
                            );
                            if let Some(fallback) = Self::find_system_fallback(worktree, &shell_env, bin_name) {
                                return Ok((fallback, vec!["lsp".to_string()], shell_env));
                            }
                            return Err(format!("下载预编译服务失败 ({download_url}): {dl_err}"));
                        }

                        zed::make_file_executable(&binary_path)?;

                        // Clean up older versions in extension work directory
                        if let Ok(entries) = fs::read_dir(".") {
                            for entry in entries.flatten() {
                                let name = entry.file_name().to_string_lossy().to_string();
                                if name.starts_with("zed-urdf-server-") && name != version_dir {
                                    let _ = fs::remove_dir_all(entry.path());
                                }
                            }
                        }
                    }

                    zed::set_language_server_installation_status(
                        language_server_id,
                        &LanguageServerInstallationStatus::None,
                    );
                    self.cached_binary_path = Some(binary_path.clone());
                    return Ok((binary_path, vec!["lsp".to_string()], shell_env));
                }
            }
            Err(_) => {
                zed::set_language_server_installation_status(
                    language_server_id,
                    &LanguageServerInstallationStatus::None,
                );
            }
        }

        // 6. Local dev fallback (`$PATH` or `~/.local/bin/zed-urdf-server` before first GitHub Release is published)
        if let Some(fallback) = Self::find_system_fallback(worktree, &shell_env, bin_name) {
            return Ok((fallback, vec!["lsp".to_string()], shell_env));
        }

        Err(format!(
            "未能自动获取 zed-urdf-server：GitHub 仓库 '{github_repo}' 尚未发布包含 '{asset_name}' 的 Release，且当前机器未找到本地预装版。"
        ))
    }

    fn find_system_fallback(
        worktree: &Worktree,
        shell_env: &[(String, String)],
        bin_name: &str,
    ) -> Option<String> {
        if let Some(found) = worktree.which(bin_name) {
            return Some(found);
        }
        if let Some((_, home)) = shell_env.iter().find(|(k, _)| k == "HOME") {
            let candidate = format!("{home}/.local/bin/{bin_name}");
            // Return candidate path on Unix dev setups where `~/.local/bin` may not be in default PATH
            return Some(candidate);
        }
        None
    }
}

impl zed::Extension for UrdfVisualizerExtension {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        let (command, args, env) = self.resolve_server_binary(language_server_id, worktree)?;
        Ok(Command { command, args, env })
    }

    fn language_server_initialization_options(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<serde_json::Value>> {
        let lsp_settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree).ok();
        if let Some(s) = lsp_settings {
            if s.initialization_options.is_some() {
                return Ok(s.initialization_options);
            }
            if s.settings.is_some() {
                return Ok(s.settings);
            }
        }
        Ok(Some(json!({
            "auto_open_viewer": false,
            "always_on_top": true
        })))
    }

    fn language_server_workspace_configuration(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<serde_json::Value>> {
        let settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)
            .ok()
            .and_then(|s| s.settings)
            .unwrap_or_else(|| json!({}));
        Ok(Some(json!({
            "urdf-lsp": settings
        })))
    }
}

zed::register_extension!(UrdfVisualizerExtension);

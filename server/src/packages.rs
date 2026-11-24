use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Default)]
pub struct PackageRegistry {
    /// Manual user overrides from Zed settings (`packages: { "pkg_name": "/path" }`)
    pub user_overrides: BTreeMap<String, PathBuf>,
    /// Auto-discovered ROS 1 / ROS 2 packages (`package_name -> package_dir`)
    pub discovered: BTreeMap<String, PathBuf>,
    /// Allowed filesystem roots for `/fs/*` relative mesh serving
    pub allowed_roots: HashSet<PathBuf>,
    /// Directories already scanned for `package.xml` so we never rescan on every keystroke
    pub scanned_dirs: HashSet<PathBuf>,
}

impl PackageRegistry {
    pub fn new() -> Self {
        let mut reg = Self::default();
        reg.scan_env_paths();
        reg
    }

    pub fn merged_map(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        for (k, v) in &self.discovered {
            out.insert(k.clone(), v.to_string_lossy().to_string());
        }
        for (k, v) in &self.user_overrides {
            out.insert(k.clone(), v.to_string_lossy().to_string());
        }
        out
    }

    pub fn resolve_package(&self, pkg_name: &str) -> Option<PathBuf> {
        self.user_overrides
            .get(pkg_name)
            .or_else(|| self.discovered.get(pkg_name))
            .cloned()
    }

    pub fn resolve_uri(&self, uri_or_path: &str, base_dir: Option<&Path>) -> Option<PathBuf> {
        let trimmed = uri_or_path.trim();
        if let Some(rest) = trimmed.strip_prefix("package://") {
            let mut parts = rest.splitn(2, '/');
            let pkg = parts.next()?;
            let rel = parts.next().unwrap_or("");
            let pkg_root = self.resolve_package(pkg)?;
            return Some(if rel.is_empty() {
                pkg_root
            } else {
                pkg_root.join(rel)
            });
        }
        if let Some(rest) = trimmed.strip_prefix("file://") {
            return Some(PathBuf::from(rest));
        }
        let p = Path::new(trimmed);
        if p.is_absolute() {
            Some(p.to_path_buf())
        } else {
            base_dir.map(|b| b.join(p))
        }
    }

    pub fn is_path_allowed(&self, candidate: &Path) -> bool {
        let Ok(canon) = candidate.canonicalize() else {
            return false;
        };
        for root in &self.allowed_roots {
            if let Ok(canon_root) = root.canonicalize() {
                if canon.starts_with(&canon_root) {
                    return true;
                }
            }
        }
        for pkg_dir in self.user_overrides.values().chain(self.discovered.values()) {
            if let Ok(canon_pkg) = pkg_dir.canonicalize() {
                if canon.starts_with(&canon_pkg) {
                    return true;
                }
            }
        }
        false
    }

    pub fn add_workspace_root(&mut self, root: &Path) {
        if !root.exists() {
            return;
        }
        self.allowed_roots.insert(root.to_path_buf());
        self.scan_directory(root, 6);
    }

    pub fn register_file_context(&mut self, file_path: &Path) {
        let Some(parent) = file_path.parent() else {
            return;
        };
        self.allowed_roots.insert(parent.to_path_buf());

        let home_dir = std::env::var_os("HOME").map(PathBuf::from);

        // Walk up to 4 ancestor directories to locate package/repo root,
        // stopping BEFORE broad user folders like ~/Downloads, ~/Desktop, ~/Git, or $HOME.
        let mut cursor = Some(parent);
        let mut highest_reasonable = parent;
        for _ in 0..4 {
            let Some(dir) = cursor else { break };
            if dir.parent().is_none() || dir == Path::new("/Users") || dir == Path::new("/home") {
                break;
            }
            if let Some(ref h) = home_dir {
                if dir == h.as_path() || dir.parent() == Some(h.as_path()) {
                    break;
                }
            }
            highest_reasonable = dir;
            if dir.join(".git").exists() || dir.join("package.xml").exists() {
                break;
            }
            cursor = dir.parent();
        }
        self.allowed_roots.insert(highest_reasonable.to_path_buf());
        self.scan_directory(highest_reasonable, 4);
    }

    pub fn apply_user_overrides(&mut self, map: BTreeMap<String, String>, workspace_root: Option<&Path>) {
        self.user_overrides.clear();
        for (name, raw_path) in map {
            let expanded = expand_variables(&raw_path, workspace_root);
            let pb = PathBuf::from(&expanded);
            let resolved = if pb.is_relative() {
                if let Some(ws) = workspace_root {
                    ws.join(pb)
                } else {
                    pb
                }
            } else {
                pb
            };
            self.allowed_roots.insert(resolved.clone());
            self.user_overrides.insert(name, resolved);
        }
    }

    pub fn scan_env_paths(&mut self) {
        // ROS 2 AMENT_PREFIX_PATH -> <prefix>/share/<pkg>/package.xml
        if let Ok(ament) = std::env::var("AMENT_PREFIX_PATH") {
            for entry in std::env::split_paths(&ament) {
                let share = entry.join("share");
                if share.is_dir() {
                    self.allowed_roots.insert(share.clone());
                    self.scan_directory(&share, 3);
                }
            }
        }
        // ROS 1 ROS_PACKAGE_PATH
        if let Ok(ros_pkg) = std::env::var("ROS_PACKAGE_PATH") {
            for entry in std::env::split_paths(&ros_pkg) {
                if entry.is_dir() {
                    self.allowed_roots.insert(entry.clone());
                    self.scan_directory(&entry, 4);
                }
            }
        }
    }

    pub fn scan_directory(&mut self, root: &Path, max_depth: usize) {
        let home_dir = std::env::var_os("HOME").map(PathBuf::from);
        if let Some(ref h) = home_dir {
            if root == h.as_path() || root.parent() == Some(h.as_path()) {
                return;
            }
        }
        if !self.scanned_dirs.insert(root.to_path_buf()) {
            return;
        }
        let walker = WalkDir::new(root)
            .max_depth(max_depth)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !matches!(
                    name.as_ref(),
                    ".git" | "node_modules" | "target" | ".venv" | "build" | "log"
                )
            });

        for entry in walker.flatten() {
            if entry.file_type().is_file() && entry.file_name() == "package.xml" {
                if let Some(pkg_dir) = entry.path().parent() {
                    if let Some(pkg_name) = parse_package_xml_name(entry.path()) {
                        self.discovered
                            .entry(pkg_name)
                            .or_insert_with(|| pkg_dir.to_path_buf());
                    }
                }
            }
        }
    }
}

fn expand_variables(raw: &str, workspace_root: Option<&Path>) -> String {
    let mut s = raw.to_string();
    if let Some(ws) = workspace_root {
        let ws_str = ws.to_string_lossy();
        s = s.replace("${workspaceFolder}", &ws_str);
        s = s.replace("${worktree}", &ws_str);
    }
    if let Ok(home) = std::env::var("HOME") {
        if let Some(rest) = s.strip_prefix("~/") {
            s = format!("{home}/{rest}");
        }
        s = s.replace("${env:HOME}", &home);
    }
    s
}

fn parse_package_xml_name(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    if let Ok(doc) = roxmltree::Document::parse(&content) {
        for child in doc.root_element().children() {
            if child.is_element() && child.tag_name().name() == "name" {
                if let Some(text) = child.text() {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
            }
        }
    }
    None
}

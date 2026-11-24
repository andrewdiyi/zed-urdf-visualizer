use crate::packages::PackageRegistry;
use roxmltree::{Document, Node};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticSeverity, DocumentSymbol, Hover, HoverContents, Location, MarkupContent,
    MarkupKind, Position, Range, SymbolKind, Url,
};

#[derive(Debug, Clone)]
pub struct LinkSummary {
    pub name: String,
    pub range: Range,
    pub mass: Option<f64>,
    pub meshes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct JointSummary {
    pub name: String,
    pub joint_type: String,
    pub parent: Option<String>,
    pub child: Option<String>,
    pub lower: Option<f64>,
    pub upper: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct AnalysisReport {
    pub diagnostics: Vec<Diagnostic>,
    pub warning_messages: Vec<String>,
    pub links: BTreeMap<String, LinkSummary>,
    pub joints: BTreeMap<String, JointSummary>,
    pub symbols: Vec<DocumentSymbol>,
    /// `(line, start_col..end_col) -> target location`
    pub ref_links: Vec<(Range, Location)>,
    pub hovers: Vec<(Range, String)>,
    pub robot_tag_range: Option<Range>,
}

pub fn analyze_urdf_or_xacro(
    raw_text: &str,
    expanded_urdf: Option<&str>,
    file_path: &Path,
    registry: &PackageRegistry,
) -> AnalysisReport {
    let mut report = AnalysisReport::default();
    let line_starts = compute_line_starts(raw_text);
    let Ok(file_uri) = Url::from_file_path(file_path) else {
        return report;
    };

    let doc = match Document::parse(raw_text) {
        Ok(d) => d,
        Err(e) => {
            let line = e.pos().row.saturating_sub(1);
            let col = e.pos().col.saturating_sub(1);
            let pos = Position::new(line, col);
            let msg = format!("XML 语法错误: {e}");
            report.warning_messages.push(msg.clone());
            report.diagnostics.push(Diagnostic {
                range: Range::new(pos, Position::new(line, col + 1)),
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("urdf-lsp".into()),
                message: msg,
                ..Default::default()
            });
            return report;
        }
    };

    let root = doc.root_element();
    let root_start = byte_offset_to_pos(root.range().start, &line_starts);
    report.robot_tag_range = Some(Range::new(
        root_start,
        Position::new(root_start.line, root_start.character + 6),
    ));
    if root.tag_name().name() != "robot" {
        let range = byte_range_to_lsp(root.range(), &line_starts);
        report.diagnostics.push(Diagnostic {
            range,
            severity: Some(DiagnosticSeverity::WARNING),
            source: Some("urdf-lsp".into()),
            message: format!("URDF 根节点通常应为 <robot>，当前为 <{}>", root.tag_name().name()),
            ..Default::default()
        });
    }

    let base_dir = file_path.parent();

    // Pass 1: Collect Links, Joints, and Mesh/Include path checks from raw document
    for node in doc.descendants().filter(|n| n.is_element()) {
        let tag = node.tag_name().name();
        match tag {
            "link" => {
                if let Some(name) = node.attribute("name") {
                    let range = byte_range_to_lsp(node.range(), &line_starts);
                    if !name.contains('$') && report.links.contains_key(name) {
                        report.diagnostics.push(Diagnostic {
                            range,
                            severity: Some(DiagnosticSeverity::ERROR),
                            source: Some("urdf-lsp".into()),
                            message: format!("重复定义的 Link 名称: '{name}'"),
                            ..Default::default()
                        });
                    }
                    let mass = node
                        .children()
                        .find(|c| c.is_element() && c.tag_name().name() == "inertial")
                        .and_then(|i| {
                            i.children()
                                .find(|m| m.is_element() && m.tag_name().name() == "mass")
                        })
                        .and_then(|m| m.attribute("value"))
                        .and_then(|v| v.parse::<f64>().ok());

                    let mut meshes = Vec::new();
                    for m in node
                        .descendants()
                        .filter(|d| d.is_element() && d.tag_name().name() == "mesh")
                    {
                        if let Some(f) = m.attribute("filename") {
                            meshes.push(f.to_string());
                        }
                    }

                    let summary = LinkSummary {
                        name: name.to_string(),
                        range,
                        mass,
                        meshes: meshes.clone(),
                    };
                    report.hovers.push((
                        range,
                        format_link_hover(&summary),
                    ));
                    #[allow(deprecated)]
                    report.symbols.push(DocumentSymbol {
                        name: format!("link: {name}"),
                        detail: mass.map(|m| format!("{m:.3} kg")),
                        kind: SymbolKind::CLASS,
                        tags: None,
                        deprecated: None,
                        range,
                        selection_range: range,
                        children: None,
                    });
                    report.links.insert(name.to_string(), summary);
                }
            }
            "joint" => {
                if let Some(name) = node.attribute("name") {
                    let jtype = node.attribute("type").unwrap_or("fixed").to_string();
                    let range = byte_range_to_lsp(node.range(), &line_starts);
                    let parent = node
                        .children()
                        .find(|c| c.is_element() && c.tag_name().name() == "parent")
                        .and_then(|p| p.attribute("link"))
                        .map(str::to_string);
                    let child = node
                        .children()
                        .find(|c| c.is_element() && c.tag_name().name() == "child")
                        .and_then(|c| c.attribute("link"))
                        .map(str::to_string);
                    let limit_node = node
                        .children()
                        .find(|c| c.is_element() && c.tag_name().name() == "limit");
                    let lower = limit_node
                        .and_then(|l| l.attribute("lower"))
                        .and_then(|v| v.parse::<f64>().ok());
                    let upper = limit_node
                        .and_then(|l| l.attribute("upper"))
                        .and_then(|v| v.parse::<f64>().ok());

                    if matches!(jtype.as_str(), "revolute" | "prismatic") && limit_node.is_none() {
                        report.diagnostics.push(Diagnostic {
                            range,
                            severity: Some(DiagnosticSeverity::WARNING),
                            source: Some("urdf-lsp".into()),
                            message: format!("{jtype} 类型关节 '{name}' 缺少 <limit> 标签"),
                            ..Default::default()
                        });
                    }
                    if let (Some(lo), Some(hi)) = (lower, upper) {
                        if lo > hi {
                            report.diagnostics.push(Diagnostic {
                                range,
                                severity: Some(DiagnosticSeverity::ERROR),
                                source: Some("urdf-lsp".into()),
                                message: format!(
                                    "关节 '{name}' 下限 lower ({lo}) 大于上限 upper ({hi})"
                                ),
                                ..Default::default()
                            });
                        }
                    }

                    let summary = JointSummary {
                        name: name.to_string(),
                        joint_type: jtype.clone(),
                        parent,
                        child,
                        lower,
                        upper,
                    };
                    report.hovers.push((range, format_joint_hover(&summary)));
                    #[allow(deprecated)]
                    report.symbols.push(DocumentSymbol {
                        name: format!("joint: {name} ({jtype})"),
                        detail: summary
                            .parent
                            .as_ref()
                            .zip(summary.child.as_ref())
                            .map(|(p, c)| format!("{p} → {c}")),
                        kind: SymbolKind::INTERFACE,
                        tags: None,
                        deprecated: None,
                        range,
                        selection_range: range,
                        children: None,
                    });
                    report.joints.insert(name.to_string(), summary);
                }
            }
            "mesh" => {
                if let Some(fname) = node.attribute("filename") {
                    if !fname.contains('$') {
                        check_mesh_path(
                            node,
                            fname,
                            base_dir,
                            registry,
                            &line_starts,
                            &mut report,
                        );
                    }
                }
            }
            "include" => {
                if let Some(fname) = node.attribute("filename") {
                    if !fname.contains('$') {
                        if let Some(inc_path) = registry.resolve_uri(fname, base_dir) {
                            if inc_path.exists() {
                                if let Ok(target_uri) = Url::from_file_path(&inc_path) {
                                    let r = byte_range_to_lsp(node.range(), &line_starts);
                                    report.ref_links.push((
                                        r,
                                        Location::new(
                                            target_uri,
                                            Range::new(Position::new(0, 0), Position::new(0, 0)),
                                        ),
                                    ));
                                }
                            } else {
                                let r = byte_range_to_lsp(node.range(), &line_starts);
                                report.diagnostics.push(Diagnostic {
                                    range: r,
                                    severity: Some(DiagnosticSeverity::ERROR),
                                    source: Some("urdf-lsp".into()),
                                    message: format!("包含文件不存在: {}", inc_path.display()),
                                    ..Default::default()
                                });
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Pass 2: Link parent/child references -> Go-to-Definition & missing link checks
    let is_pure_urdf = expanded_urdf.is_none();
    for node in doc.descendants().filter(|n| n.is_element()) {
        let tag = node.tag_name().name();
        if matches!(tag, "parent" | "child") {
            if let Some(link_name) = node.attribute("link") {
                if link_name.contains('$') {
                    continue;
                }
                let r = byte_range_to_lsp(node.range(), &line_starts);
                if let Some(target_link) = report.links.get(link_name) {
                    report.ref_links.push((
                        r,
                        Location::new(file_uri.clone(), target_link.range),
                    ));
                } else if is_pure_urdf {
                    report.diagnostics.push(Diagnostic {
                        range: r,
                        severity: Some(DiagnosticSeverity::ERROR),
                        source: Some("urdf-lsp".into()),
                        message: format!("<{tag}> 引用了未定义的 Link: '{link_name}'"),
                        ..Default::default()
                    });
                }
            }
        }
    }

    // Pass 3: If Xacro expanded into URDF, validate expanded mesh paths too
    if let Some(urdf_str) = expanded_urdf {
        if let Ok(exp_doc) = Document::parse(urdf_str) {
            for mesh_node in exp_doc
                .descendants()
                .filter(|n| n.is_element() && n.tag_name().name() == "mesh")
            {
                if let Some(fname) = mesh_node.attribute("filename") {
                    match registry.resolve_uri(fname, base_dir) {
                        Some(p) if !p.exists() => {
                            let msg = format!("Xacro 展开后网格文件缺失: {fname} (解析路径: {})", p.display());
                            if !report.warning_messages.contains(&msg) {
                                report.warning_messages.push(msg);
                            }
                        }
                        None => {
                            let msg = format!("无法解析 ROS 包路径: {fname}");
                            if !report.warning_messages.contains(&msg) {
                                report.warning_messages.push(msg);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    report
}

fn check_mesh_path(
    node: Node,
    fname: &str,
    base_dir: Option<&Path>,
    registry: &PackageRegistry,
    line_starts: &[usize],
    report: &mut AnalysisReport,
) {
    let range = byte_range_to_lsp(node.range(), line_starts);
    match registry.resolve_uri(fname, base_dir) {
        Some(resolved) => {
            if !resolved.exists() {
                let msg = format!("网格文件不存在: {}", resolved.display());
                report.warning_messages.push(msg.clone());
                report.diagnostics.push(Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    source: Some("urdf-lsp".into()),
                    message: msg,
                    ..Default::default()
                });
            } else {
                report.hovers.push((
                    range,
                    format!("**Mesh Asset**\n\n- URI: `{fname}`\n- 本地路径: `{}`", resolved.display()),
                ));
            }
        }
        None => {
            let msg = format!(
                "未找到 ROS Package 映射: '{fname}'。可在 .zed/settings.json 的 lsp.urdf-lsp.settings.packages 中手动指定路径"
            );
            report.warning_messages.push(msg.clone());
            report.diagnostics.push(Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::WARNING),
                source: Some("urdf-lsp".into()),
                message: msg,
                ..Default::default()
            });
        }
    }
}

fn format_link_hover(link: &LinkSummary) -> String {
    let mut md = format!("### Link `{}`\n", link.name);
    if let Some(m) = link.mass {
        md.push_str(&format!("- **质量 (Mass):** `{m:.4} kg`\n"));
    }
    if !link.meshes.is_empty() {
        md.push_str("- **网格资源:**\n");
        for m in &link.meshes {
            md.push_str(&format!("  - `{m}`\n"));
        }
    }
    md
}

fn format_joint_hover(j: &JointSummary) -> String {
    let mut md = format!("### Joint `{}` (`{}`)\n", j.name, j.joint_type);
    if let (Some(p), Some(c)) = (&j.parent, &j.child) {
        md.push_str(&format!("- **运动学链:** `{p}` → `{c}`\n"));
    }
    if let (Some(lo), Some(hi)) = (j.lower, j.upper) {
        if j.joint_type == "prismatic" {
            md.push_str(&format!("- **行程限位:** `[{lo:.4} m, {hi:.4} m]`\n"));
        } else {
            md.push_str(&format!(
                "- **角度限位:** `[{lo:.4}, {hi:.4}] rad` (`[{:.1}°, {:.1}°]`)\n",
                lo.to_degrees(),
                hi.to_degrees()
            ));
        }
    }
    md
}

pub fn find_symbol_line_col(file_path: &PathBuf, kind: &str, name: &str) -> Option<(u32, u32)> {
    let content = std::fs::read_to_string(file_path).ok()?;
    let line_starts = compute_line_starts(&content);
    let doc = Document::parse(&content).ok()?;
    for node in doc.descendants().filter(|n| n.is_element()) {
        if node.tag_name().name() == kind && node.attribute("name") == Some(name) {
            let r = byte_range_to_lsp(node.range(), &line_starts);
            return Some((r.start.line + 1, r.start.character + 1));
        }
    }
    None
}

pub fn position_in_range(pos: Position, range: Range) -> bool {
    if pos.line < range.start.line || pos.line > range.end.line {
        return false;
    }
    if pos.line == range.start.line && pos.character < range.start.character {
        return false;
    }
    if pos.line == range.end.line && pos.character > range.end.character {
        return false;
    }
    true
}

pub fn make_markdown_hover(md: String, range: Range) -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: md,
        }),
        range: Some(range),
    }
}

fn compute_line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

fn byte_offset_to_pos(offset: usize, line_starts: &[usize]) -> Position {
    let line_idx = match line_starts.binary_search(&offset) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    };
    let col = offset.saturating_sub(line_starts[line_idx]);
    Position::new(line_idx as u32, col as u32)
}

fn byte_range_to_lsp(range: std::ops::Range<usize>, line_starts: &[usize]) -> Range {
    Range::new(
        byte_offset_to_pos(range.start, line_starts),
        byte_offset_to_pos(range.end, line_starts),
    )
}

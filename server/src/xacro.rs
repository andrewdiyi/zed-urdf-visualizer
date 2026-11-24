use crate::packages::PackageRegistry;
use anyhow::{anyhow, bail, Result};
use roxmltree::{Document, Node};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct XacroResult {
    pub urdf_xml: String,
    pub discovered_args: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
struct MacroDef {
    params: Vec<(String, Option<String>)>,
    /// Raw XML children snippet or cloned subtree representation
    body_xml: String,
}

#[derive(Debug, Clone)]
enum PropVal {
    Scalar(String),
    Block(String),
}

struct EvalCtx<'a> {
    registry: &'a PackageRegistry,
    args: BTreeMap<String, String>,
    discovered_args: BTreeMap<String, String>,
    props: BTreeMap<String, PropVal>,
    macros: BTreeMap<String, MacroDef>,
    include_depth: usize,
}

pub fn is_xacro_document(path: &Path, content: &str) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if name.ends_with(".xacro") {
        return true;
    }
    content.contains("xmlns:xacro") || content.contains("<xacro:")
}

pub fn process_xacro(
    content: &str,
    file_path: &Path,
    registry: &PackageRegistry,
    arg_overrides: &BTreeMap<String, String>,
    custom_xacro_cmd: Option<&str>,
) -> Result<XacroResult> {
    // Discover xacro:arg declarations first so UI can always edit them
    let mut discovered_args = extract_declared_args(content);
    for (k, v) in arg_overrides {
        discovered_args.insert(k.clone(), v.clone());
    }

    // 1. Try external `xacro` CLI if available (when content matches disk or via temp file in same dir)
    if let Some(cli_xml) = try_external_xacro(content, file_path, &discovered_args, custom_xacro_cmd) {
        return Ok(XacroResult {
            urdf_xml: cli_xml,
            discovered_args,
        });
    }

    // 2. Built-in pure-Rust Xacro evaluator (zero Python/ROS dependencies required!)
    let mut ctx = EvalCtx {
        registry,
        args: discovered_args.clone(),
        discovered_args,
        props: BTreeMap::new(),
        macros: BTreeMap::new(),
        include_depth: 0,
    };
    ctx.props
        .insert("pi".to_string(), PropVal::Scalar(std::f64::consts::PI.to_string()));
    ctx.props
        .insert("tau".to_string(), PropVal::Scalar(std::f64::consts::TAU.to_string()));

    let base_dir = file_path.parent().unwrap_or_else(|| Path::new("."));
    let expanded = expand_document_str(content, base_dir, &mut ctx, true)?;

    Ok(XacroResult {
        urdf_xml: expanded,
        discovered_args: ctx.discovered_args,
    })
}

fn extract_declared_args(content: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Ok(doc) = Document::parse(content) {
        for node in doc.descendants() {
            if node.is_element() && is_xacro_tag(node, "arg") {
                if let Some(name) = node.attribute("name") {
                    let default_val = node.attribute("default").unwrap_or("").to_string();
                    out.entry(name.to_string()).or_insert(default_val);
                }
            }
        }
    }
    out
}

fn try_external_xacro(
    content: &str,
    file_path: &Path,
    args: &BTreeMap<String, String>,
    custom_cmd: Option<&str>,
) -> Option<String> {
    let bin = custom_cmd.unwrap_or("xacro");
    // Quick check if binary exists
    Command::new(bin).arg("--version").output().ok()?;

    // Write unsaved buffer to a hidden sibling temp file so relative includes still work
    let parent = file_path.parent().unwrap_or_else(|| Path::new("/tmp"));
    let stem = file_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("model.xacro");
    let tmp_path = parent.join(format!(".zed_urdf_preview_{}_{}", std::process::id(), stem));
    let target_path: PathBuf = if std::fs::write(&tmp_path, content).is_ok() {
        tmp_path.clone()
    } else if file_path.exists() {
        file_path.to_path_buf()
    } else {
        return None;
    };

    let mut cmd = Command::new(bin);
    cmd.arg(&target_path);
    for (k, v) in args {
        if !v.is_empty() {
            cmd.arg(format!("{k}:={v}"));
        }
    }
    let output = cmd.output();
    let _ = std::fs::remove_file(&tmp_path);

    let out = output.ok()?;
    if out.status.success() {
        String::from_utf8(out.stdout).ok()
    } else {
        None
    }
}

fn is_xacro_tag(node: Node, local: &str) -> bool {
    if !node.is_element() {
        return false;
    }
    let tag = node.tag_name().name();
    if tag == local {
        if let Some(ns) = node.tag_name().namespace() {
            if ns.contains("xacro") {
                return true;
            }
        }
    }
    tag == format!("xacro:{local}")
}

fn xacro_local_name<'a>(node: Node<'a, 'a>) -> Option<&'a str> {
    if !node.is_element() {
        return None;
    }
    let name = node.tag_name().name();
    if let Some(ns) = node.tag_name().namespace() {
        if ns.contains("xacro") {
            return Some(name);
        }
    }
    name.strip_prefix("xacro:")
}

fn expand_document_str(
    xml_text: &str,
    base_dir: &Path,
    ctx: &mut EvalCtx,
    is_root: bool,
) -> Result<String> {
    if ctx.include_depth > 32 {
        bail!("Xacro include/macro 嵌套超过上限 (32 层)");
    }
    let doc = Document::parse(xml_text)
        .map_err(|e| anyhow!("Xacro XML 语法错误 (行 {}, 列 {}): {}", e.pos().row, e.pos().col, e))?;
    let root = doc.root_element();

    let mut out = String::new();
    if is_root {
        out.push_str("<?xml version=\"1.0\"?>\n");
        let robot_name = root
            .attribute("name")
            .map(|v| eval_substitutions(v, base_dir, ctx))
            .transpose()?
            .unwrap_or_else(|| "robot".to_string());
        out.push_str(&format!("<robot name=\"{}\">\n", escape_xml_attr(&robot_name)));
        for child in root.children() {
            expand_node(child, xml_text, base_dir, ctx, &BTreeMap::new(), &mut out)?;
        }
        out.push_str("</robot>\n");
    } else {
        for child in root.children() {
            expand_node(child, xml_text, base_dir, ctx, &BTreeMap::new(), &mut out)?;
        }
    }
    Ok(out)
}

fn expand_node(
    node: Node,
    source_xml: &str,
    base_dir: &Path,
    ctx: &mut EvalCtx,
    block_params: &BTreeMap<String, (bool, String)>,
    out: &mut String,
) -> Result<()> {
    if node.is_text() {
        if let Some(t) = node.text() {
            let expanded = eval_substitutions(t, base_dir, ctx)?;
            out.push_str(&escape_xml_text(&expanded));
        }
        return Ok(());
    }
    if !node.is_element() {
        return Ok(());
    }

    if let Some(xtag) = xacro_local_name(node) {
        match xtag {
            "arg" => {
                if let Some(name) = node.attribute("name") {
                    let def = node.attribute("default").unwrap_or("");
                    let eval_def = eval_substitutions(def, base_dir, ctx)?;
                    ctx.discovered_args
                        .entry(name.to_string())
                        .or_insert_with(|| eval_def.clone());
                    ctx.args.entry(name.to_string()).or_insert(eval_def);
                }
                return Ok(());
            }
            "property" => {
                if let Some(name) = node.attribute("name") {
                    if let Some(val) = node.attribute("value") {
                        let ev = eval_substitutions(val, base_dir, ctx)?;
                        ctx.props.insert(name.to_string(), PropVal::Scalar(ev));
                    } else {
                        let inner = slice_children_raw(node, source_xml);
                        ctx.props.insert(name.to_string(), PropVal::Block(inner));
                    }
                }
                return Ok(());
            }
            "include" => {
                if let Some(fname) = node.attribute("filename") {
                    let resolved_str = eval_substitutions(fname, base_dir, ctx)?;
                    let path = ctx
                        .registry
                        .resolve_uri(&resolved_str, Some(base_dir))
                        .ok_or_else(|| anyhow!("无法解析 xacro:include 路径: {resolved_str}"))?;
                    let inc_text = std::fs::read_to_string(&path).map_err(|e| {
                        anyhow!("无法读取 xacro:include 文件 {}: {e}", path.display())
                    })?;
                    let inc_dir = path.parent().unwrap_or(base_dir);
                    ctx.include_depth += 1;
                    let inc_out = expand_document_str(&inc_text, inc_dir, ctx, false)?;
                    ctx.include_depth -= 1;
                    out.push_str(&inc_out);
                }
                return Ok(());
            }
            "if" | "unless" => {
                let cond_raw = node.attribute("value").unwrap_or("0");
                let cond_str = eval_substitutions(cond_raw, base_dir, ctx)?;
                let truthy = is_truthy(&cond_str);
                let should_run = if xtag == "if" { truthy } else { !truthy };
                if should_run {
                    for child in node.children() {
                        expand_node(child, source_xml, base_dir, ctx, block_params, out)?;
                    }
                }
                return Ok(());
            }
            "insert_block" => {
                if let Some(bname) = node.attribute("name") {
                    if let Some((insert_children_only, raw_xml)) = block_params.get(bname) {
                        let wrapped = format!(
                            "<root xmlns:xacro=\"http://www.ros.org/wiki/xacro\">{raw_xml}</root>"
                        );
                        let sub_doc = Document::parse(&wrapped)?;
                        if *insert_children_only {
                            if let Some(first_el) =
                                sub_doc.root_element().children().find(|n| n.is_element())
                            {
                                for grand in first_el.children() {
                                    expand_node(grand, &wrapped, base_dir, ctx, block_params, out)?;
                                }
                            }
                        } else {
                            for child in sub_doc.root_element().children() {
                                expand_node(child, &wrapped, base_dir, ctx, block_params, out)?;
                            }
                        }
                    } else if let Some(PropVal::Block(raw_xml)) = ctx.props.get(bname).cloned() {
                        let wrapped = format!(
                            "<root xmlns:xacro=\"http://www.ros.org/wiki/xacro\">{raw_xml}</root>"
                        );
                        let sub_doc = Document::parse(&wrapped)?;
                        for child in sub_doc.root_element().children() {
                            expand_node(child, &wrapped, base_dir, ctx, block_params, out)?;
                        }
                    }
                }
                return Ok(());
            }
            "macro" => {
                if let Some(mname) = node.attribute("name") {
                    let params_attr = node.attribute("params").unwrap_or("");
                    let params = parse_macro_params(params_attr);
                    let body_xml = slice_children_raw(node, source_xml);
                    ctx.macros
                        .insert(mname.to_string(), MacroDef { params, body_xml });
                }
                return Ok(());
            }
            other => {
                if let Some(mdef) = ctx.macros.get(other).cloned() {
                    invoke_macro(&mdef, node, source_xml, base_dir, ctx, block_params, out)?;
                    return Ok(());
                }
            }
        }
    }

    // Check unprefixed macro call if registered
    let raw_tag = node.tag_name().name();
    if let Some(mdef) = ctx.macros.get(raw_tag).cloned() {
        invoke_macro(&mdef, node, source_xml, base_dir, ctx, block_params, out)?;
        return Ok(());
    }

    // Standard URDF element emission
    out.push('<');
    out.push_str(raw_tag);
    for attr in node.attributes() {
        if attr.name().starts_with("xmlns:xacro") {
            continue;
        }
        let val = eval_substitutions(attr.value(), base_dir, ctx)?;
        out.push(' ');
        out.push_str(attr.name());
        out.push_str("=\"");
        out.push_str(&escape_xml_attr(&val));
        out.push('"');
    }

    if !node.has_children() {
        out.push_str("/>");
    } else {
        out.push('>');
        for child in node.children() {
            expand_node(child, source_xml, base_dir, ctx, block_params, out)?;
        }
        out.push_str("</");
        out.push_str(raw_tag);
        out.push('>');
    }
    Ok(())
}

fn invoke_macro(
    mdef: &MacroDef,
    call_node: Node,
    source_xml: &str,
    base_dir: &Path,
    ctx: &mut EvalCtx,
    caller_blocks: &BTreeMap<String, (bool, String)>,
    out: &mut String,
) -> Result<()> {
    let saved_props = ctx.props.clone();
    let mut local_blocks = caller_blocks.clone();
    let mut element_children = call_node.children().filter(|c| c.is_element());

    for (pname, default_val) in &mdef.params {
        if let Some(block_name) = pname.strip_prefix("**") {
            if let Some(child_el) = element_children.next() {
                let raw = &source_xml[child_el.range()];
                local_blocks.insert(block_name.to_string(), (true, raw.to_string()));
            }
        } else if let Some(block_name) = pname.strip_prefix('*') {
            if let Some(child_el) = element_children.next() {
                let raw = &source_xml[child_el.range()];
                local_blocks.insert(block_name.to_string(), (false, raw.to_string()));
            }
        } else {
            let val = if let Some(attr_val) = call_node.attribute(pname.as_str()) {
                eval_substitutions(attr_val, base_dir, ctx)?
            } else if let Some(def) = default_val {
                let cleaned = def.trim_matches('\'').trim_matches('"');
                eval_substitutions(cleaned, base_dir, ctx)?
            } else {
                String::new()
            };
            ctx.props.insert(pname.clone(), PropVal::Scalar(val));
        }
    }

    let wrapped = format!(
        "<root xmlns:xacro=\"http://www.ros.org/wiki/xacro\">{}</root>",
        mdef.body_xml
    );
    let sub_doc = Document::parse(&wrapped)?;
    ctx.include_depth += 1;
    for child in sub_doc.root_element().children() {
        expand_node(child, &wrapped, base_dir, ctx, &local_blocks, out)?;
    }
    ctx.include_depth -= 1;

    ctx.props = saved_props;
    Ok(())
}

fn slice_children_raw(node: Node, source_xml: &str) -> String {
    let mut buf = String::new();
    for c in node.children() {
        buf.push_str(&source_xml[c.range()]);
    }
    buf
}

fn parse_macro_params(params_str: &str) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    for token in params_str.split_whitespace() {
        if let Some((k, v)) = token.split_once(":=") {
            out.push((k.to_string(), Some(v.to_string())));
        } else if let Some((k, v)) = token.split_once('=') {
            out.push((k.to_string(), Some(v.to_string())));
        } else {
            out.push((token.to_string(), None));
        }
    }
    out
}

fn is_truthy(val: &str) -> bool {
    let t = val.trim().to_ascii_lowercase();
    !matches!(t.as_str(), "" | "0" | "0.0" | "false" | "none")
}

fn eval_substitutions(input: &str, _base_dir: &Path, ctx: &EvalCtx) -> Result<String> {
    if !input.contains('$') {
        return Ok(input.to_string());
    }
    let mut result = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'$' {
                result.push('$');
                i += 2;
                continue;
            }
            if bytes[i + 1] == b'{' {
                if let Some((inner, next_idx)) = extract_balanced(&input[i + 2..], '{', '}') {
                    let evaluated = eval_expr(inner.trim(), ctx);
                    result.push_str(&evaluated);
                    i += 2 + next_idx;
                    continue;
                }
            }
            if bytes[i + 1] == b'(' {
                if let Some((inner, next_idx)) = extract_balanced(&input[i + 2..], '(', ')') {
                    let evaluated = eval_dollar_command(inner.trim(), ctx)?;
                    result.push_str(&evaluated);
                    i += 2 + next_idx;
                    continue;
                }
            }
        }
        let ch = input[i..].chars().next().unwrap();
        result.push(ch);
        i += ch.len_utf8();
    }
    Ok(result)
}

fn extract_balanced(s: &str, open: char, close: char) -> Option<(&str, usize)> {
    let mut depth = 1usize;
    for (idx, ch) in s.char_indices() {
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                return Some((&s[..idx], idx + ch.len_utf8()));
            }
        }
    }
    None
}

fn eval_dollar_command(cmd: &str, ctx: &EvalCtx) -> Result<String> {
    let mut parts = cmd.split_whitespace();
    let Some(head) = parts.next() else {
        return Ok(String::new());
    };
    let arg = parts.next().unwrap_or("");
    match head {
        "find" => {
            if let Some(pkg_path) = ctx.registry.resolve_package(arg) {
                Ok(pkg_path.to_string_lossy().to_string())
            } else {
                // Preserve package:// URI so browser URDFLoader can still attempt /pkg/<arg>
                Ok(format!("package://{arg}"))
            }
        }
        "arg" => Ok(ctx.args.get(arg).cloned().unwrap_or_default()),
        "env" | "optenv" => Ok(std::env::var(arg)
            .ok()
            .unwrap_or_else(|| parts.collect::<Vec<_>>().join(" "))),
        _ => Ok(String::new()),
    }
}

/// Evaluates `${...}` math/boolean/property expressions in Xacro
fn eval_expr(expr: &str, ctx: &EvalCtx) -> String {
    // Direct property lookup first (preserves string properties like link prefixes)
    if let Some(PropVal::Scalar(v)) = ctx.props.get(expr) {
        return v.clone();
    }

    let mut parser = MathParser::new(expr, ctx);
    if let Some(val) = parser.parse_full() {
        if (val - val.round()).abs() < 1e-12 && val.abs() < 1e15 {
            if expr.contains('.') {
                format!("{val:.6}").trim_end_matches('0').trim_end_matches('.').to_string()
            } else {
                format!("{}", val.round() as i64)
            }
        } else {
            format!("{val}")
        }
    } else {
        // String fallback: substitute known identifiers if quoted/concatenated
        substitute_identifiers_as_string(expr, ctx)
    }
}

fn substitute_identifiers_as_string(expr: &str, ctx: &EvalCtx) -> String {
    let trimmed = expr.trim();
    if (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        || (trimmed.starts_with('"') && trimmed.ends_with('"'))
    {
        return trimmed[1..trimmed.len() - 1].to_string();
    }
    let mut out = expr.to_string();
    for (k, v) in &ctx.props {
        if let PropVal::Scalar(sv) = v {
            if out == *k {
                return sv.clone();
            }
            out = out.replace(&format!("'{k}'"), &format!("'{sv}'"));
        }
    }
    out
}

struct MathParser<'a> {
    chars: Vec<char>,
    pos: usize,
    ctx: &'a EvalCtx<'a>,
}

impl<'a> MathParser<'a> {
    fn new(s: &str, ctx: &'a EvalCtx<'a>) -> Self {
        Self {
            chars: s.chars().collect(),
            pos: 0,
            ctx,
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
            self.pos += 1;
        }
    }

    fn parse_full(&mut self) -> Option<f64> {
        let v = self.parse_or()?;
        self.skip_ws();
        if self.pos == self.chars.len() {
            Some(v)
        } else {
            None
        }
    }

    fn match_kw(&mut self, kw: &str) -> bool {
        self.skip_ws();
        let kw_chars: Vec<char> = kw.chars().collect();
        if self.pos + kw_chars.len() > self.chars.len() {
            return false;
        }
        for (i, c) in kw_chars.iter().enumerate() {
            if self.chars[self.pos + i] != *c {
                return false;
            }
        }
        let after = self.pos + kw_chars.len();
        if after < self.chars.len()
            && (self.chars[after].is_alphanumeric() || self.chars[after] == '_')
        {
            return false;
        }
        self.pos = after;
        true
    }

    fn match_op(&mut self, op: &str) -> bool {
        self.skip_ws();
        let op_chars: Vec<char> = op.chars().collect();
        if self.pos + op_chars.len() > self.chars.len() {
            return false;
        }
        for (i, c) in op_chars.iter().enumerate() {
            if self.chars[self.pos + i] != *c {
                return false;
            }
        }
        self.pos += op_chars.len();
        true
    }

    fn parse_or(&mut self) -> Option<f64> {
        let mut left = self.parse_and()?;
        while self.match_kw("or") {
            let right = self.parse_and()?;
            left = if left != 0.0 || right != 0.0 { 1.0 } else { 0.0 };
        }
        Some(left)
    }

    fn parse_and(&mut self) -> Option<f64> {
        let mut left = self.parse_cmp()?;
        while self.match_kw("and") {
            let right = self.parse_cmp()?;
            left = if left != 0.0 && right != 0.0 { 1.0 } else { 0.0 };
        }
        Some(left)
    }

    fn parse_cmp(&mut self) -> Option<f64> {
        let save = self.pos;
        if let Some((lhs_s, op, rhs_s)) = self.try_string_equality() {
            let eq = lhs_s == rhs_s;
            return Some(if (op == "==" && eq) || (op == "!=" && !eq) {
                1.0
            } else {
                0.0
            });
        }
        self.pos = save;

        let left = self.parse_add()?;
        for op in ["==", "!=", "<=", ">=", "<", ">"] {
            if self.match_op(op) {
                let right = self.parse_add()?;
                let res = match op {
                    "==" => (left - right).abs() < 1e-9,
                    "!=" => (left - right).abs() >= 1e-9,
                    "<=" => left <= right,
                    ">=" => left >= right,
                    "<" => left < right,
                    ">" => left > right,
                    _ => false,
                };
                return Some(if res { 1.0 } else { 0.0 });
            }
        }
        Some(left)
    }

    fn try_string_equality(&mut self) -> Option<(String, &'static str, String)> {
        let lhs = self.parse_string_atom()?;
        let op = if self.match_op("==") {
            "=="
        } else if self.match_op("!=") {
            "!="
        } else {
            return None;
        };
        let rhs = self.parse_string_atom()?;
        Some((lhs, op, rhs))
    }

    fn parse_string_atom(&mut self) -> Option<String> {
        self.skip_ws();
        if self.pos >= self.chars.len() {
            return None;
        }
        let quote = self.chars[self.pos];
        if quote == '\'' || quote == '"' {
            self.pos += 1;
            let start = self.pos;
            while self.pos < self.chars.len() && self.chars[self.pos] != quote {
                self.pos += 1;
            }
            if self.pos >= self.chars.len() {
                return None;
            }
            let s: String = self.chars[start..self.pos].iter().collect();
            self.pos += 1;
            return Some(s);
        }
        let ident = self.read_ident()?;
        if let Some(PropVal::Scalar(v)) = self.ctx.props.get(&ident) {
            return Some(v.clone());
        }
        None
    }

    fn parse_add(&mut self) -> Option<f64> {
        let mut left = self.parse_mul()?;
        loop {
            self.skip_ws();
            if self.pos < self.chars.len() && self.chars[self.pos] == '+' {
                self.pos += 1;
                left += self.parse_mul()?;
            } else if self.pos < self.chars.len() && self.chars[self.pos] == '-' {
                self.pos += 1;
                left -= self.parse_mul()?;
            } else {
                break;
            }
        }
        Some(left)
    }

    fn parse_mul(&mut self) -> Option<f64> {
        let mut left = self.parse_pow()?;
        loop {
            self.skip_ws();
            if self.pos + 1 < self.chars.len()
                && self.chars[self.pos] == '*'
                && self.chars[self.pos + 1] == '*'
            {
                break;
            }
            if self.pos < self.chars.len() && self.chars[self.pos] == '*' {
                self.pos += 1;
                left *= self.parse_pow()?;
            } else if self.pos < self.chars.len() && self.chars[self.pos] == '/' {
                self.pos += 1;
                let denom = self.parse_pow()?;
                if denom.abs() < 1e-15 {
                    return None;
                }
                left /= denom;
            } else if self.pos < self.chars.len() && self.chars[self.pos] == '%' {
                self.pos += 1;
                let denom = self.parse_pow()?;
                left %= denom;
            } else {
                break;
            }
        }
        Some(left)
    }

    fn parse_pow(&mut self) -> Option<f64> {
        let base = self.parse_unary()?;
        if self.match_op("**") {
            let exp = self.parse_pow()?;
            Some(base.powf(exp))
        } else {
            Some(base)
        }
    }

    fn parse_unary(&mut self) -> Option<f64> {
        self.skip_ws();
        if self.match_kw("not") {
            let v = self.parse_unary()?;
            return Some(if v == 0.0 { 1.0 } else { 0.0 });
        }
        if self.pos < self.chars.len() && self.chars[self.pos] == '-' {
            self.pos += 1;
            return Some(-self.parse_unary()?);
        }
        if self.pos < self.chars.len() && self.chars[self.pos] == '+' {
            self.pos += 1;
            return self.parse_unary();
        }
        self.parse_primary()
    }

    fn read_ident(&mut self) -> Option<String> {
        self.skip_ws();
        let start = self.pos;
        if start >= self.chars.len()
            || !(self.chars[start].is_alphabetic() || self.chars[start] == '_')
        {
            return None;
        }
        while self.pos < self.chars.len()
            && (self.chars[self.pos].is_alphanumeric()
                || self.chars[self.pos] == '_'
                || self.chars[self.pos] == '.')
        {
            self.pos += 1;
        }
        Some(self.chars[start..self.pos].iter().collect())
    }

    fn parse_primary(&mut self) -> Option<f64> {
        self.skip_ws();
        if self.pos >= self.chars.len() {
            return None;
        }
        if self.chars[self.pos] == '(' {
            self.pos += 1;
            let v = self.parse_or()?;
            self.skip_ws();
            if self.pos >= self.chars.len() || self.chars[self.pos] != ')' {
                return None;
            }
            self.pos += 1;
            return Some(v);
        }

        // Number literal
        if self.chars[self.pos].is_ascii_digit() || self.chars[self.pos] == '.' {
            let start = self.pos;
            while self.pos < self.chars.len()
                && (self.chars[self.pos].is_ascii_digit() || self.chars[self.pos] == '.')
            {
                self.pos += 1;
            }
            if self.pos < self.chars.len()
                && (self.chars[self.pos] == 'e' || self.chars[self.pos] == 'E')
            {
                self.pos += 1;
                if self.pos < self.chars.len()
                    && (self.chars[self.pos] == '+' || self.chars[self.pos] == '-')
                {
                    self.pos += 1;
                }
                while self.pos < self.chars.len() && self.chars[self.pos].is_ascii_digit() {
                    self.pos += 1;
                }
            }
            let num_str: String = self.chars[start..self.pos].iter().collect();
            return num_str.parse::<f64>().ok();
        }

        let ident = self.read_ident()?;
        let clean_ident = ident.strip_prefix("math.").unwrap_or(&ident);
        self.skip_ws();

        // Function call
        if self.pos < self.chars.len() && self.chars[self.pos] == '(' {
            self.pos += 1;
            let arg1 = self.parse_or()?;
            self.skip_ws();
            let mut arg2 = None;
            if self.pos < self.chars.len() && self.chars[self.pos] == ',' {
                self.pos += 1;
                arg2 = Some(self.parse_or()?);
                self.skip_ws();
            }
            if self.pos >= self.chars.len() || self.chars[self.pos] != ')' {
                return None;
            }
            self.pos += 1;
            return match clean_ident {
                "sin" => Some(arg1.sin()),
                "cos" => Some(arg1.cos()),
                "tan" => Some(arg1.tan()),
                "asin" => Some(arg1.asin()),
                "acos" => Some(arg1.acos()),
                "atan" => Some(arg1.atan()),
                "atan2" => Some(arg1.atan2(arg2?)),
                "sqrt" => Some(arg1.sqrt()),
                "abs" | "fabs" => Some(arg1.abs()),
                "radians" => Some(arg1.to_radians()),
                "degrees" => Some(arg1.to_degrees()),
                "pow" => Some(arg1.powf(arg2?)),
                _ => None,
            };
        }

        match clean_ident {
            "true" | "True" => Some(1.0),
            "false" | "False" => Some(0.0),
            "pi" => Some(std::f64::consts::PI),
            "tau" => Some(std::f64::consts::TAU),
            _ => {
                if let Some(PropVal::Scalar(v)) = self.ctx.props.get(&ident) {
                    v.trim().parse::<f64>().ok()
                } else {
                    None
                }
            }
        }
    }
}

fn escape_xml_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn escape_xml_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

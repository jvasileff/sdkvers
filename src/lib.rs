use std::collections::HashMap;
use std::env;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self(value.to_string())
    }
}

fn err(message: impl Into<String>) -> Error {
    Error(message.into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Separator {
    Start,
    Dot,
    Dash,
    Underscore,
}

impl Separator {
    pub fn as_str(self) -> &'static str {
        match self {
            Separator::Start => "start",
            Separator::Dot => "dot",
            Separator::Dash => "dash",
            Separator::Underscore => "underscore",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtomKind {
    Digit,
    Alpha,
    Other,
}

impl AtomKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AtomKind::Digit => "digit",
            AtomKind::Alpha => "alpha",
            AtomKind::Other => "other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Atom {
    pub text: String,
    pub kind: AtomKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component {
    pub separator: Separator,
    pub atoms: Vec<Atom>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionNode {
    pub source: String,
    pub components: Vec<Component>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionExprNode {
    Exact {
        source: String,
        version: VersionNode,
    },
    Range {
        source: String,
        lower_inclusive: bool,
        lower: Option<VersionNode>,
        upper: Option<VersionNode>,
        upper_inclusive: bool,
    },
}

impl VersionExprNode {
    pub fn source(&self) -> &str {
        match self {
            VersionExprNode::Exact { source, .. } => source,
            VersionExprNode::Range { source, .. } => source,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigLineNode {
    pub line_number: usize,
    pub source: String,
    pub candidate: String,
    pub expr: VersionExprNode,
    pub vendor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentLineNode {
    pub line_number: usize,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentNode {
    pub entries: Vec<DocumentLineNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdkListRow {
    pub candidate: String,
    pub version: String,
    pub vendor_label: Option<String>,
    pub dist: Option<String>,
    pub status: Option<String>,
    pub identifier: Option<String>,
    pub in_use: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdkListNode {
    pub candidate: String,
    pub rows: Vec<SdkListRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRow {
    pub candidate: String,
    pub version: String,
    pub target: String,
    pub vendor_label: Option<String>,
    pub dist: Option<String>,
    pub status: Option<String>,
    pub in_use: bool,
}

pub struct VersionParser<'a> {
    source: &'a str,
    index: usize,
}

impl<'a> VersionParser<'a> {
    pub fn new(source: &'a str) -> Self {
        Self { source, index: 0 }
    }

    pub fn parse_version(mut self) -> Result<VersionNode> {
        if self.source.is_empty() {
            return Err(err("empty version string"));
        }
        let version = self.parse_version_at_current()?;
        if !self.at_end() {
            return Err(self.error("unexpected trailing input"));
        }
        Ok(version)
    }

    pub fn parse_version_expr(mut self) -> Result<VersionExprNode> {
        if self.source.is_empty() {
            return Err(err("empty version expression"));
        }
        if matches!(self.peek_char(), Some('[' | '(')) {
            self.parse_bracketed_version_expr()
        } else {
            let version = self.parse_version()?;
            Ok(Self::bare_version_expr(version)?)
        }
    }

    // Expands a bare version to its canonical range form:
    //   1 numeric segment ("21")      → [21,22)   major-line range
    //   2 numeric segments ("3.9")    → [3.9,3.10) minor-line range
    //   3+ numeric segments ("8.7.0") → [8.7.0]    exact match
    // Any version containing a letter or underscore is always treated as exact,
    // regardless of segment count, because mixed tokens are too irregular to
    // interpret as a prefix range.
    fn bare_version_expr(version: VersionNode) -> Result<VersionExprNode> {
        let segment_count = Self::pure_numeric_segment_count(&version);
        if segment_count == 1 {
            let upper = VersionParser::new(&Self::next_major(&version.source)?).parse_version()?;
            return Ok(VersionExprNode::Range {
                source: version.source.clone(),
                lower_inclusive: true,
                lower: Some(version),
                upper: Some(upper),
                upper_inclusive: false,
            });
        }
        if segment_count == 2 {
            let upper = VersionParser::new(&Self::next_minor(&version.source)?).parse_version()?;
            return Ok(VersionExprNode::Range {
                source: version.source.clone(),
                lower_inclusive: true,
                lower: Some(version),
                upper: Some(upper),
                upper_inclusive: false,
            });
        }
        Ok(VersionExprNode::Exact {
            source: version.source.clone(),
            version,
        })
    }

    fn pure_numeric_segment_count(version: &VersionNode) -> usize {
        for component in &version.components {
            if component.atoms.len() != 1 {
                return 0;
            }
            if component.atoms[0].kind != AtomKind::Digit {
                return 0;
            }
        }
        version.components.len()
    }

    fn next_major(text: &str) -> Result<String> {
        let value: i64 = text
            .parse()
            .map_err(|_| err(format!("invalid major version: {text}")))?;
        Ok((value + 1).to_string())
    }

    fn next_minor(text: &str) -> Result<String> {
        let dot = text
            .find('.')
            .ok_or_else(|| err(format!("invalid minor version: {text}")))?;
        let major_text = &text[..dot];
        let minor_text = &text[dot + 1..];
        let major: i64 = major_text
            .parse()
            .map_err(|_| err(format!("invalid major version: {major_text}")))?;
        let minor: i64 = minor_text
            .parse()
            .map_err(|_| err(format!("invalid minor version: {minor_text}")))?;
        Ok(format!("{major}.{}", minor + 1))
    }

    // Parses [a,b], [a,b), (a,b], (a,b), [a,), (,b], (,b), [a].
    // '['/']' = inclusive bound, '('/')' = exclusive bound.
    // A single version inside square brackets with no comma is an exact match: [a].
    fn parse_bracketed_version_expr(&mut self) -> Result<VersionExprNode> {
        let start_index = self.index;
        let lower_inclusive = match self.current_char()? {
            '[' => true,
            '(' => false,
            _ => return Err(self.error("expected '[' or '('")),
        };
        self.advance()?;

        let lower = if matches!(self.peek_char(), Some(',' | ')' | ']')) {
            None
        } else {
            Some(self.parse_version_at_current()?)
        };

        if matches!(self.peek_char(), Some(']')) {
            self.advance()?;
            if lower.is_none() {
                return Err(self.error("invalid exact expression"));
            }
            if !self.at_end() {
                return Err(self.error("unexpected trailing input"));
            }
            let source = self.source[start_index..self.index].to_string();
            return Ok(VersionExprNode::Exact {
                source,
                version: lower.unwrap(),
            });
        }

        self.consume_char(',')?;

        let upper = if matches!(self.peek_char(), Some(')' | ']')) {
            None
        } else {
            Some(self.parse_version_at_current()?)
        };

        let upper_inclusive = match self.current_char()? {
            ']' => true,
            ')' => false,
            _ => return Err(self.error("expected ']' or ')'")),
        };
        self.advance()?;

        if !self.at_end() {
            return Err(self.error("unexpected trailing input"));
        }

        Ok(VersionExprNode::Range {
            source: self.source[start_index..self.index].to_string(),
            lower_inclusive,
            lower,
            upper,
            upper_inclusive,
        })
    }

    fn parse_version_at_current(&mut self) -> Result<VersionNode> {
        if self.at_end() {
            return Err(self.error("expected version"));
        }
        let start_index = self.index;
        let mut components = Vec::new();
        self.parse_component_sequence(&mut components, Separator::Start)?;
        if components.is_empty() {
            return Err(self.error("expected version"));
        }
        Ok(VersionNode {
            source: self.source[start_index..self.index].to_string(),
            components,
        })
    }

    fn parse_component_sequence(
        &mut self,
        components: &mut Vec<Component>,
        separator: Separator,
    ) -> Result<()> {
        let component = self.parse_component(separator)?;
        components.push(component);

        if self.at_end() || self.is_version_stop_char(self.current_char()?) {
            return Ok(());
        }

        if let Some(separator) = self.parse_separator()? {
            self.parse_component_sequence(components, separator)?;
        }
        Ok(())
    }

    fn parse_component(&mut self, separator: Separator) -> Result<Component> {
        let mut atoms = Vec::new();
        let atom = self.parse_atom()?;
        atoms.push(atom);
        loop {
            if self.at_end() {
                break;
            }
            let ch = self.current_char()?;
            if self.is_separator_char(ch) || self.is_version_stop_char(ch) {
                break;
            }
            if self.atom_boundary(&atoms[atoms.len() - 1], ch) {
                atoms.push(self.parse_atom()?);
            } else {
                let len = atoms.len();
                atoms[len - 1].text.push(ch);
                self.advance()?;
            }
        }
        Ok(Component { separator, atoms })
    }

    fn parse_atom(&mut self) -> Result<Atom> {
        let ch = self.current_char()?;
        let kind = Self::classify_char(ch);
        let mut text = String::new();
        text.push(ch);
        self.advance()?;
        Ok(Atom { text, kind })
    }

    fn parse_separator(&mut self) -> Result<Option<Separator>> {
        if self.at_end() {
            return Ok(None);
        }
        let separator = match self.current_char()? {
            '.' => Separator::Dot,
            '-' => Separator::Dash,
            '_' => Separator::Underscore,
            _ => return Ok(None),
        };
        self.advance()?;
        Ok(Some(separator))
    }

    fn atom_boundary(&self, previous: &Atom, next: char) -> bool {
        matches!(
            (previous.kind, Self::classify_char(next)),
            (AtomKind::Digit, AtomKind::Alpha)
                | (AtomKind::Alpha, AtomKind::Digit)
                | (AtomKind::Other, AtomKind::Digit)
                | (AtomKind::Other, AtomKind::Alpha)
                | (AtomKind::Digit, AtomKind::Other)
                | (AtomKind::Alpha, AtomKind::Other)
        )
    }

    fn classify_char(ch: char) -> AtomKind {
        if ch.is_ascii_digit() {
            AtomKind::Digit
        } else if ch.is_ascii_alphabetic() {
            AtomKind::Alpha
        } else {
            AtomKind::Other
        }
    }

    fn is_version_stop_char(&self, ch: char) -> bool {
        matches!(ch, ',' | ']' | ')')
    }

    fn is_separator_char(&self, ch: char) -> bool {
        matches!(ch, '.' | '-' | '_')
    }

    fn peek_char(&self) -> Option<char> {
        self.source[self.index..].chars().next()
    }

    fn current_char(&self) -> Result<char> {
        self.peek_char()
            .ok_or_else(|| self.error("unexpected end of input"))
    }

    fn advance(&mut self) -> Result<()> {
        let ch = self.current_char()?;
        self.index += ch.len_utf8();
        Ok(())
    }

    fn consume_char(&mut self, expected: char) -> Result<()> {
        let actual = self.current_char()?;
        if actual != expected {
            return Err(self.error(format!("expected [{expected}]")));
        }
        self.advance()
    }

    fn at_end(&self) -> bool {
        self.index >= self.source.len()
    }

    fn error(&self, message: impl fmt::Display) -> Error {
        err(format!("{message} at offset {} in [{}]", self.index, self.source))
    }
}

pub struct ConfigLineParser<'a> {
    source: &'a str,
    line_number: usize,
    index: usize,
}

impl<'a> ConfigLineParser<'a> {
    pub fn new(source: &'a str, line_number: usize) -> Self {
        Self {
            source,
            line_number,
            index: 0,
        }
    }

    pub fn parse_line(mut self) -> Result<ConfigLineNode> {
        self.skip_whitespace()?;
        if self.at_end() {
            return Err(self.error("empty config line"));
        }
        let candidate = self.parse_candidate()?;
        self.skip_whitespace()?;
        self.consume_char('=')?;
        self.skip_whitespace()?;
        let expr_text = self.parse_expr_text()?;
        let expr = VersionParser::new(&expr_text).parse_version_expr()?;
        self.skip_whitespace()?;
        let vendor = if self.at_end() {
            None
        } else {
            let vendor = self.parse_vendor()?;
            self.skip_whitespace()?;
            if !self.at_end() {
                return Err(self.error("unexpected trailing input"));
            }
            Some(vendor)
        };
        Ok(ConfigLineNode {
            line_number: self.line_number,
            source: self.source.to_string(),
            candidate,
            expr,
            vendor,
        })
    }

    fn parse_candidate(&mut self) -> Result<String> {
        let start = self.index;
        while !self.at_end() {
            let ch = self.current_char()?;
            if ch.is_whitespace() || ch == '=' {
                break;
            }
            self.advance()?;
        }
        if start == self.index {
            return Err(self.error("missing candidate before '='"));
        }
        Ok(self.source[start..self.index].to_string())
    }

    fn parse_expr_text(&mut self) -> Result<String> {
        if self.at_end() {
            return Err(self.error("missing version expression after '='"));
        }
        let start = self.index;
        while !self.at_end() && !self.current_char()?.is_whitespace() {
            self.advance()?;
        }
        if start == self.index {
            return Err(self.error("missing version expression after '='"));
        }
        Ok(self.source[start..self.index].to_string())
    }

    fn parse_vendor(&mut self) -> Result<String> {
        let start = self.index;
        while !self.at_end() && !self.current_char()?.is_whitespace() {
            self.advance()?;
        }
        if start == self.index {
            return Err(self.error("expected vendor"));
        }
        Ok(self.source[start..self.index].to_string())
    }

    fn skip_whitespace(&mut self) -> Result<()> {
        while !self.at_end() && self.current_char()?.is_whitespace() {
            self.advance()?;
        }
        Ok(())
    }

    fn peek_char(&self) -> Option<char> {
        self.source[self.index..].chars().next()
    }

    fn current_char(&self) -> Result<char> {
        self.peek_char()
            .ok_or_else(|| self.error("unexpected end of line"))
    }

    fn advance(&mut self) -> Result<()> {
        let ch = self.current_char()?;
        self.index += ch.len_utf8();
        Ok(())
    }

    fn consume_char(&mut self, expected: char) -> Result<()> {
        let actual = self.current_char()?;
        if actual != expected {
            return Err(self.error(format!("expected [{expected}]")));
        }
        self.advance()
    }

    fn at_end(&self) -> bool {
        self.index >= self.source.len()
    }

    fn error(&self, message: impl fmt::Display) -> Error {
        err(format!(
            "{message} at line {} offset {} in [{}]",
            self.line_number, self.index, self.source
        ))
    }
}

pub fn parse_document(source: &str) -> DocumentNode {
    let mut entries = Vec::new();
    for (idx, raw_line) in source.lines().enumerate() {
        let trimmed = raw_line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            entries.push(DocumentLineNode {
                line_number: idx + 1,
                source: raw_line.to_string(),
            });
        }
    }
    DocumentNode { entries }
}

// SDKMAN produces two distinct listing formats. Java uses a pipe-delimited table
// with a vendor column ("| Use | Version |"). All other candidates use a
// fixed-width grid of version strings with status marker prefixes.
pub fn parse_sdk_list(candidate: &str, source: &str) -> SdkListNode {
    let rows = if source.contains("| Use | Version") {
        parse_java_table(candidate, source)
    } else {
        parse_generic_grid(candidate, source)
    };
    SdkListNode {
        candidate: candidate.to_string(),
        rows,
    }
}

fn parse_java_table(candidate: &str, source: &str) -> Vec<SdkListRow> {
    let mut rows = Vec::new();
    let mut current_vendor: Option<String> = None;
    for raw_line in source.lines() {
        if raw_line.contains('|') && !is_java_header_line(raw_line) {
            let parsed_vendor = trim_string(extract_pipe_field(raw_line, 0));
            if !parsed_vendor.is_empty() {
                current_vendor = Some(parsed_vendor);
            }
            let version = trim_string(extract_pipe_field(raw_line, 2));
            if version.is_empty() {
                continue;
            }
            let dist = trim_optional(extract_pipe_field(raw_line, 3));
            let status = trim_optional(extract_pipe_field(raw_line, 4));
            let identifier = trim_optional(extract_pipe_field(raw_line, 5));
            let use_text = trim_string(extract_pipe_field(raw_line, 1));
            rows.push(SdkListRow {
                candidate: candidate.to_string(),
                version,
                vendor_label: current_vendor.clone(),
                dist,
                status,
                identifier,
                in_use: use_text.contains('>'),
            });
        }
    }
    rows
}

// The generic grid format lays out versions in fixed-width columns:
// the first column is 25 characters wide, subsequent columns are 20 characters.
// Each cell may begin with marker characters (' ', '>', '*', '+') indicating
// in-use and install status, followed by the version string.
fn parse_generic_grid(candidate: &str, source: &str) -> Vec<SdkListRow> {
    let mut rows = Vec::new();
    for raw_line in source.lines() {
        let trimmed = raw_line.trim();
        if !is_generic_grid_data_line(trimmed) {
            continue;
        }
        let mut cell_start = 0usize;
        let line_len = raw_line.len();
        while cell_start < line_len {
            let cell_end = if cell_start == 0 {
                (cell_start + 25).min(line_len)
            } else {
                (cell_start + 20).min(line_len)
            };
            let cell = &raw_line[cell_start..cell_end];
            let marker_end = find_generic_marker_end(cell);
            let marker_text = &cell[..marker_end];
            let version_text = cell[marker_end..].trim().to_string();
            if !version_text.is_empty() {
                let status = if marker_text.contains('*') && marker_text.contains('>') {
                    Some("current installed".to_string())
                } else if marker_text.contains('*') {
                    Some("installed".to_string())
                } else if marker_text.contains('+') && marker_text.contains('>') {
                    Some("current local only".to_string())
                } else if marker_text.contains('+') {
                    Some("local only".to_string())
                } else {
                    None
                };
                rows.push(SdkListRow {
                    candidate: candidate.to_string(),
                    version: version_text,
                    vendor_label: None,
                    dist: None,
                    status,
                    identifier: None,
                    in_use: marker_text.contains('>'),
                });
            }
            cell_start = if cell_start == 0 {
                25
            } else {
                cell_start + 20
            };
        }
    }
    rows
}

fn find_generic_marker_end(cell: &str) -> usize {
    for (idx, ch) in cell.char_indices() {
        if !matches!(ch, ' ' | '>' | '*' | '+') {
            return idx;
        }
    }
    cell.len()
}

fn is_java_header_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.is_empty()
        || trimmed.starts_with('=')
        || trimmed.starts_with('-')
        || trimmed.starts_with("Vendor")
        || trimmed.starts_with("Available ")
}

fn is_generic_grid_data_line(trimmed: &str) -> bool {
    !(trimmed.is_empty()
        || trimmed.starts_with('=')
        || trimmed.starts_with('+')
        || trimmed.starts_with('*')
        || trimmed.starts_with("> -")
        || trimmed.starts_with("Available "))
}

fn extract_pipe_field(line: &str, field_index: usize) -> &str {
    line.split('|').nth(field_index).unwrap_or("")
}

fn trim_string(text: &str) -> String {
    text.trim().to_string()
}

fn trim_optional(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub fn load_local_sdk_list(candidate: &str) -> Result<SdkListNode> {
    let candidates_root = sdkman_candidates_path()?;
    let candidate_path = candidates_root.join(candidate);
    if !candidate_path.is_dir() {
        return Ok(SdkListNode {
            candidate: candidate.to_string(),
            rows: Vec::new(),
        });
    }
    let current = current_identifier(&candidate_path);
    let mut rows = Vec::new();
    for entry in fs::read_dir(&candidate_path)? {
        let entry = entry?;
        let path = entry.path();
        let identifier = entry.file_name().to_string_lossy().to_string();
        if identifier == "current" || !path.is_dir() {
            continue;
        }
        let in_use = current.as_deref() == Some(identifier.as_str());
        let status = if in_use {
            Some("current local only".to_string())
        } else {
            Some("local only".to_string())
        };
        rows.push(SdkListRow {
            candidate: candidate.to_string(),
            version: inferred_version(candidate, &identifier),
            vendor_label: None,
            dist: inferred_dist(candidate, &identifier),
            status,
            identifier: Some(identifier),
            in_use,
        });
    }
    Ok(SdkListNode {
        candidate: candidate.to_string(),
        rows,
    })
}

fn sdkman_candidates_path() -> Result<PathBuf> {
    if let Ok(dir) = env::var("SDKMAN_DIR") {
        return Ok(Path::new(&dir).join("candidates"));
    }
    let home = env::var("HOME").map_err(|_| err("could not determine home directory for SDKMAN lookup"))?;
    Ok(Path::new(&home).join(".sdkman").join("candidates"))
}

fn current_identifier(candidate_path: &Path) -> Option<String> {
    let current_path = candidate_path.join("current");
    if !current_path.exists() {
        return None;
    }
    fs::read_link(current_path)
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()).or_else(|| Some(p.to_string_lossy().to_string())))
}

fn inferred_version(candidate: &str, identifier: &str) -> String {
    if candidate == "java" {
        if let Some(idx) = identifier.rfind('-') {
            if idx > 0 {
                return identifier[..idx].to_string();
            }
        }
    }
    identifier.to_string()
}

// Java identifiers have the form "<version>-<dist>" (e.g. "21.0.2-tem").
// We split on the last hyphen to extract the distribution label.
// Non-java candidates don't have a dist field.
fn inferred_dist(candidate: &str, identifier: &str) -> Option<String> {
    if candidate != "java" {
        return None;
    }
    let idx = identifier.rfind('-')?;
    if idx + 1 < identifier.len() {
        Some(identifier[idx + 1..].to_string())
    } else {
        None
    }
}

pub fn run_sdk_list(candidate: &str) -> Result<String> {
    let init_path = sdkman_init_path()?;
    let script = format!(
        ". '{}' >/dev/null 2>&1; sdk list '{}'",
        shell_escape_single_quoted(&init_path),
        shell_escape_single_quoted(candidate)
    );
    let output = Command::new("bash")
        .arg("-c")
        .arg(script)
        .output()
        .map_err(|e| err(format!("failed to run sdk list {candidate}: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            return Err(err(format!(
                "sdk list {candidate} failed with exit code {}",
                output.status.code().unwrap_or(1)
            )));
        }
        return Err(err(format!(
            "sdk list {candidate} failed with exit code {}: {stderr}",
            output.status.code().unwrap_or(1)
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn sdkman_init_path() -> Result<String> {
    if let Ok(dir) = env::var("SDKMAN_DIR") {
        return Ok(format!("{dir}/bin/sdkman-init.sh"));
    }
    let home = env::var("HOME").map_err(|_| err("could not determine home directory for SDKMAN lookup"))?;
    Ok(format!("{home}/.sdkman/bin/sdkman-init.sh"))
}

// Escapes a string for use inside single quotes in a POSIX shell command.
// Single quotes cannot be escaped inside single quotes; the technique is to
// end the single-quoted string, insert a double-quoted single quote, then
// reopen the single-quoted string: ' → '"'"'
fn shell_escape_single_quoted(text: &str) -> String {
    text.replace('\'', "'\"'\"'")
}

// Each token in a flattened version string is assigned a role that determines
// how it sorts relative to a position with no token (the plain release endpoint).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnitRole {
    Numeric,      // digit sequence; compares numerically
    Prerelease,   // alpha/beta/rc/ea/etc; sorts before the plain release
    ReleaseAlias, // final/ga/release; treated as equal to the plain release (skipped)
    Variant,      // fx/crac; sorts after the plain release
    Unknown,      // unrecognized qualifier; sorts after the plain release
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ComparisonUnit {
    role: UnitRole,
    text: String,
    numeric_value: i64,
}

#[derive(Default)]
pub struct Resolver;

impl Resolver {
    pub fn resolve_line(&self, line: &ConfigLineNode, sdk_list: &SdkListNode) -> Result<ResolvedRow> {
        if line.candidate != sdk_list.candidate {
            return Err(err(format!(
                "candidate mismatch: {} vs {}",
                line.candidate, sdk_list.candidate
            )));
        }
        let mut best: Option<&SdkListRow> = None;
        let mut saw_local = false;
        for row in &sdk_list.rows {
            if !Self::is_local_status(row.status.as_deref()) {
                continue;
            }
            saw_local = true;
            if !self.vendor_matches(line, row) {
                continue;
            }
            if !self.version_expr_matches(&line.expr, &row.version)? {
                continue;
            }
            if let Some(current_best) = best {
                if self.compare_versions(&row.version, &current_best.version)? > 0 {
                    best = Some(row);
                }
            } else {
                best = Some(row);
            }
        }
        if !saw_local {
            return Err(err(format!(
                "no installed versions found for tool: {}",
                line.candidate
            )));
        }
        let matched = if let Some(best) = best {
            best
        } else if let Some(vendor) = &line.vendor {
            return Err(err(format!(
                "no installed version of {} matches {} with vendor {}",
                line.candidate,
                line.expr.source(),
                vendor
            )));
        } else {
            return Err(err(format!(
                "no installed version of {} matches {}",
                line.candidate,
                line.expr.source()
            )));
        };
        let target = matched
            .identifier
            .clone()
            .unwrap_or_else(|| matched.version.clone());
        Ok(ResolvedRow {
            candidate: matched.candidate.clone(),
            version: matched.version.clone(),
            target,
            vendor_label: matched.vendor_label.clone(),
            dist: matched.dist.clone(),
            status: matched.status.clone(),
            in_use: matched.in_use,
        })
    }

    fn is_local_status(status: Option<&str>) -> bool {
        matches!(
            status,
            Some("installed" | "current installed" | "local only" | "current local only")
        )
    }

    fn vendor_matches(&self, line: &ConfigLineNode, row: &SdkListRow) -> bool {
        match (&line.vendor, &row.dist) {
            (Some(requested), Some(dist)) => requested == dist,
            (Some(_), None) => false,
            (None, _) => true,
        }
    }

    // Compares a version string against a parsed VersionNode using unit comparison
    // only (no string tiebreaker), so release aliases compare equal to their plain
    // counterpart: semantic_eq("2.16.0.Final", node_for("2.16.0")) == true.
    fn semantic_eq(&self, version_str: &str, node: &VersionNode) -> Result<bool> {
        let parsed = VersionParser::new(version_str).parse_version()?;
        let left = self.flatten_version(&parsed)?;
        let right = self.flatten_version(node)?;
        Ok(self.compare_units(&left, 0, &right, 0)? == 0)
    }

    pub fn version_expr_matches(&self, expr: &VersionExprNode, version: &str) -> Result<bool> {
        if !self.prerelease_allowed_for_expr(expr, version)? {
            return Ok(false);
        }
        match expr {
            VersionExprNode::Exact { version: exact, .. } => {
                self.semantic_eq(version, exact)
            }
            VersionExprNode::Range {
                lower_inclusive,
                lower,
                upper,
                upper_inclusive,
                ..
            } => {
                if let Some(lower) = lower {
                    let cmp = self.compare_versions(version, &lower.source)?;
                    if cmp < 0 || (cmp == 0 && !lower_inclusive) {
                        return Ok(false);
                    }
                }
                if let Some(upper) = upper {
                    let cmp = self.compare_versions(version, &upper.source)?;
                    if cmp > 0 || (cmp == 0 && !upper_inclusive) {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
        }
    }

    // Pre-release candidates are excluded by default. They are allowed only when
    // a bound explicitly contains a pre-release qualifier, and only for that same
    // base line. For example, [26.ea,) allows 26.ea.* but not 27.ea.*.
    // Stable candidates always pass this filter regardless of bounds.
    fn prerelease_allowed_for_expr(&self, expr: &VersionExprNode, candidate_version: &str) -> Result<bool> {
        if let VersionExprNode::Exact { version, .. } = expr {
            // Use semantic comparison (no string tiebreaker) so release aliases
            // like 2.16.0.Final are eligible for exact match against [2.16.0].
            return self.semantic_eq(candidate_version, version);
        }
        let candidate = VersionParser::new(candidate_version).parse_version()?;
        let candidate_base = self.prerelease_base(&candidate)?;
        let Some(candidate_base) = candidate_base else {
            return Ok(true);
        };
        let lower_base = self.lower_prerelease_base(expr)?;
        let upper_base = self.upper_prerelease_base(expr)?;
        if lower_base.is_none() && upper_base.is_none() {
            return Ok(false);
        }
        Ok(lower_base.as_deref() == Some(candidate_base.as_str())
            || upper_base.as_deref() == Some(candidate_base.as_str()))
    }

    fn lower_prerelease_base(&self, expr: &VersionExprNode) -> Result<Option<String>> {
        match expr {
            VersionExprNode::Range { lower, .. } => lower.as_ref().map(|v| self.prerelease_base(v)).transpose().map(|o| o.flatten()),
            _ => Ok(None),
        }
    }

    fn upper_prerelease_base(&self, expr: &VersionExprNode) -> Result<Option<String>> {
        match expr {
            VersionExprNode::Range { upper, .. } => upper.as_ref().map(|v| self.prerelease_base(v)).transpose().map(|o| o.flatten()),
            _ => Ok(None),
        }
    }

    // Returns the numeric prefix before the first pre-release component, or None
    // if the version has no pre-release component. E.g. "26.ea.35" → Some("26"),
    // "9.4.0-rc-1" → Some("9.4.0"), "21.0.2" → None.
    fn prerelease_base(&self, version: &VersionNode) -> Result<Option<String>> {
        let Some(index) = self.first_prerelease_component_index(version) else {
            return Ok(None);
        };
        let mut parts = Vec::new();
        for component in &version.components[..index] {
            let text = self.numeric_component_text(component);
            if !text.is_empty() {
                parts.push(text);
            }
        }
        Ok(Some(parts.join(".")))
    }

    fn first_prerelease_component_index(&self, version: &VersionNode) -> Option<usize> {
        version
            .components
            .iter()
            .position(|component| self.is_prerelease_component(component))
    }

    fn is_prerelease_component(&self, component: &Component) -> bool {
        component.atoms.iter().any(|atom| {
            atom.kind == AtomKind::Alpha && is_prerelease_qualifier(&atom.text.to_ascii_lowercase())
        })
    }

    fn numeric_component_text(&self, component: &Component) -> String {
        component
            .atoms
            .iter()
            .filter(|atom| atom.kind == AtomKind::Digit)
            .map(|atom| atom.text.as_str())
            .collect::<String>()
    }

    pub fn compare_versions(&self, left: &str, right: &str) -> Result<i32> {
        let left_version = VersionParser::new(left).parse_version()?;
        let right_version = VersionParser::new(right).parse_version()?;
        self.compare_version_nodes(&left_version, &right_version)
    }

    fn compare_version_nodes(&self, left: &VersionNode, right: &VersionNode) -> Result<i32> {
        let left_units = self.flatten_version(left)?;
        let right_units = self.flatten_version(right)?;
        let unit_cmp = self.compare_units(&left_units, 0, &right_units, 0)?;
        if unit_cmp != 0 {
            return Ok(unit_cmp);
        }
        Ok(cmp_i32(left.source.cmp(&right.source)))
    }

    fn flatten_version(&self, version: &VersionNode) -> Result<Vec<ComparisonUnit>> {
        let mut units = Vec::new();
        for component in &version.components {
            for atom in &component.atoms {
                units.push(self.make_unit(atom)?);
            }
        }
        Ok(units)
    }

    fn make_unit(&self, atom: &Atom) -> Result<ComparisonUnit> {
        if atom.kind == AtomKind::Digit {
            let numeric_value = atom
                .text
                .parse::<i64>()
                .map_err(|_| err(format!("invalid numeric token: {}", atom.text)))?;
            return Ok(ComparisonUnit {
                role: UnitRole::Numeric,
                text: atom.text.to_ascii_lowercase(),
                numeric_value,
            });
        }
        let lower = atom.text.to_ascii_lowercase();
        let (role, numeric_value) = if is_prerelease_qualifier(&lower) {
            (UnitRole::Prerelease, qualifier_precedence(&lower))
        } else if is_release_alias(&lower) {
            (UnitRole::ReleaseAlias, 0)
        } else if is_variant(&lower) {
            (UnitRole::Variant, 0)
        } else {
            (UnitRole::Unknown, 0)
        };
        Ok(ComparisonUnit {
            role,
            text: lower,
            numeric_value,
        })
    }

    // Compares two token sequences left-to-right, skipping release-alias tokens.
    // When one side is exhausted: if the other side's next token is a pre-release
    // qualifier, the exhausted side wins (plain release > pre-release); otherwise
    // the exhausted side loses (variant/unknown tokens sort after the plain release).
    fn compare_units(
        &self,
        left_units: &[ComparisonUnit],
        left_index: usize,
        right_units: &[ComparisonUnit],
        right_index: usize,
    ) -> Result<i32> {
        let next_left = self.next_effective_index(left_units, left_index);
        let next_right = self.next_effective_index(right_units, right_index);
        if next_left >= left_units.len() && next_right >= right_units.len() {
            return Ok(0);
        }
        if next_left >= left_units.len() {
            let right = &right_units[next_right];
            return Ok(if right.role == UnitRole::Prerelease { 1 } else { -1 });
        }
        if next_right >= right_units.len() {
            let left = &left_units[next_left];
            return Ok(if left.role == UnitRole::Prerelease { -1 } else { 1 });
        }
        let left = &left_units[next_left];
        let right = &right_units[next_right];
        let unit_cmp = self.compare_unit_values(left, right);
        if unit_cmp != 0 {
            return Ok(unit_cmp);
        }
        self.compare_units(left_units, next_left + 1, right_units, next_right + 1)
    }

    fn next_effective_index(&self, units: &[ComparisonUnit], start: usize) -> usize {
        let mut idx = start;
        while idx < units.len() && units[idx].role == UnitRole::ReleaseAlias {
            idx += 1;
        }
        idx
    }

    fn compare_unit_values(&self, left: &ComparisonUnit, right: &ComparisonUnit) -> i32 {
        if left.role == right.role {
            if left.role == UnitRole::Numeric || left.role == UnitRole::Prerelease {
                return cmp_i32(left.numeric_value.cmp(&right.numeric_value));
            }
            return cmp_i32(left.text.cmp(&right.text));
        }
        if left.role == UnitRole::Numeric {
            return if right.role == UnitRole::Prerelease { 1 } else { -1 };
        }
        if right.role == UnitRole::Numeric {
            return if left.role == UnitRole::Prerelease { -1 } else { 1 };
        }
        cmp_i32(role_rank(left.role).cmp(&role_rank(right.role)))
    }
}

fn cmp_i32(ordering: std::cmp::Ordering) -> i32 {
    match ordering {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

fn role_rank(role: UnitRole) -> i32 {
    match role {
        UnitRole::Prerelease => 1,
        UnitRole::Variant => 3,
        UnitRole::Unknown => 5,
        _ => 2,
    }
}

// Pre-release ordering: alpha < beta < milestone < rc < ea < preview < snapshot < (release)
fn is_prerelease_qualifier(value: &str) -> bool {
    matches!(
        value,
        "alpha" | "a" | "beta" | "b" | "milestone" | "m" | "rc" | "cr" | "ea" | "preview" | "snapshot"
    )
}

// Release aliases compare equal to the plain release (skipped during comparison).
fn is_release_alias(value: &str) -> bool {
    matches!(value, "final" | "ga" | "release")
}

// Variant qualifiers represent packaging/feature differences (not instability)
// and sort after the plain release.
fn is_variant(value: &str) -> bool {
    matches!(value, "fx" | "crac")
}

fn qualifier_precedence(value: &str) -> i64 {
    match value {
        "alpha" | "a" => 1,
        "beta" | "b" => 2,
        "milestone" | "m" => 3,
        "rc" | "cr" => 4,
        "ea" => 5,
        "preview" => 6,
        "snapshot" => 7,
        _ => 100,
    }
}

pub fn read_utf8_file(path: &str) -> Result<String> {
    fs::read_to_string(path).map_err(|e| err(format!("failed to read file: {path}: {e}")))
}

pub fn find_sdkvers_path(start_path: &str) -> Result<String> {
    let requested = fs::canonicalize(start_path).unwrap_or_else(|_| PathBuf::from(start_path));
    let mut current = if requested.is_dir() {
        requested.clone()
    } else {
        requested
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or(requested.clone())
    };
    loop {
        let candidate = current.join(".sdkvers");
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
        if !current.pop() {
            return Err(err(format!(
                "no .sdkvers file found from: {}",
                requested.to_string_lossy()
            )));
        }
    }
}

pub fn resolve_document(path: &str) -> Result<Vec<String>> {
    let (commands, errors) = resolve_document_with_details(path)?;
    if errors.is_empty() {
        Ok(commands)
    } else {
        Err(err(format!("{}\n{}", commands.join("\n"), errors.join("\n"))))
    }
}

pub fn resolve_document_with_details(path: &str) -> Result<(Vec<String>, Vec<String>)> {
    let document = parse_document(&read_utf8_file(path)?);
    let mut cache: HashMap<String, SdkListNode> = HashMap::new();
    let resolver = Resolver;
    let mut commands = Vec::new();
    let mut errors = Vec::new();
    for entry in document.entries {
        match ConfigLineParser::new(&entry.source, entry.line_number)
            .parse_line()
            .and_then(|config| {
                let sdk_list = if let Some(existing) = cache.get(&config.candidate) {
                    existing.clone()
                } else {
                    let parsed = load_local_sdk_list(&config.candidate)?;
                    cache.insert(config.candidate.clone(), parsed.clone());
                    parsed
                };
                resolver.resolve_line(&config, &sdk_list)
            }) {
            Ok(row) => commands.push(format!("sdk use {} {}", row.candidate, row.target)),
            Err(e) => errors.push(format!("error: {} (line {} in {})", e.0, entry.line_number, path)),
        }
    }
    Ok((commands, errors))
}

pub fn dump_version(node: &VersionNode) -> String {
    let mut out = String::new();
    out.push_str(&format!("source={}\n", node.source));
    out.push_str(&format!("component_count={}\n", node.components.len()));
    for (component_index, component) in node.components.iter().enumerate() {
        let i = component_index + 1;
        out.push_str(&format!(
            "component[{}].separator={}\n",
            i,
            component.separator.as_str()
        ));
        out.push_str(&format!(
            "component[{}].atom_count={}\n",
            i,
            component.atoms.len()
        ));
        for (atom_index, atom) in component.atoms.iter().enumerate() {
            let j = atom_index + 1;
            out.push_str(&format!("component[{}].atom[{}].text={}\n", i, j, atom.text));
            out.push_str(&format!(
                "component[{}].atom[{}].kind={}\n",
                i,
                j,
                atom.kind.as_str()
            ));
        }
    }
    out
}

pub fn dump_version_expr(node: &VersionExprNode) -> String {
    let mut out = String::new();
    match node {
        VersionExprNode::Exact { source, version } => {
            out.push_str("expr_kind=exact\n");
            out.push_str(&format!("expr_source={source}\n"));
            out.push_str(&dump_version(version));
        }
        VersionExprNode::Range {
            source,
            lower_inclusive,
            lower,
            upper,
            upper_inclusive,
        } => {
            out.push_str("expr_kind=range\n");
            out.push_str(&format!("expr_source={source}\n"));
            out.push_str(&format!("lower_inclusive={lower_inclusive}\n"));
            if let Some(lower) = lower {
                out.push_str("lower.present=true\n");
                out.push_str(&format!("lower.source={}\n", lower.source));
                out.push_str(&format!("lower.component_count={}\n", lower.components.len()));
            } else {
                out.push_str("lower.present=false\n");
            }
            if let Some(upper) = upper {
                out.push_str("upper.present=true\n");
                out.push_str(&format!("upper.source={}\n", upper.source));
                out.push_str(&format!("upper.component_count={}\n", upper.components.len()));
            } else {
                out.push_str("upper.present=false\n");
            }
            out.push_str(&format!("upper_inclusive={upper_inclusive}\n"));
        }
    }
    out
}

pub fn dump_config_line(node: &ConfigLineNode) -> String {
    let mut out = String::new();
    out.push_str(&format!("line_number={}\n", node.line_number));
    out.push_str(&format!("candidate={}\n", node.candidate));
    if let Some(vendor) = &node.vendor {
        out.push_str("vendor.present=true\n");
        out.push_str(&format!("vendor={vendor}\n"));
    } else {
        out.push_str("vendor.present=false\n");
    }
    out.push_str(&dump_version_expr(&node.expr));
    out
}

pub fn dump_document(node: &DocumentNode) -> String {
    let mut out = String::new();
    out.push_str(&format!("entry_count={}\n", node.entries.len()));
    for (idx, entry) in node.entries.iter().enumerate() {
        let i = idx + 1;
        out.push_str(&format!("entry[{}].begin=true\n", i));
        out.push_str(&format!("entry[{}].line_number={}\n", i, entry.line_number));
        out.push_str(&format!("entry[{}].source={}\n", i, entry.source));
        match ConfigLineParser::new(&entry.source, entry.line_number).parse_line() {
            Ok(config) => {
                out.push_str(&format!("entry[{}].candidate={}\n", i, config.candidate));
                if let Some(vendor) = &config.vendor {
                    out.push_str(&format!("entry[{}].vendor.present=true\n", i));
                    out.push_str(&format!("entry[{}].vendor={}\n", i, vendor));
                } else {
                    out.push_str(&format!("entry[{}].vendor.present=false\n", i));
                }
                match &config.expr {
                    VersionExprNode::Exact { source, version } => {
                        out.push_str(&format!("entry[{}].expr_kind=exact\n", i));
                        out.push_str(&format!("entry[{}].expr_source={}\n", i, source));
                        out.push_str(&format!(
                            "entry[{}].expr.version.source={}\n",
                            i, version.source
                        ));
                        out.push_str(&format!(
                            "entry[{}].expr.version.component_count={}\n",
                            i,
                            version.components.len()
                        ));
                    }
                    VersionExprNode::Range {
                        source,
                        lower_inclusive,
                        lower,
                        upper,
                        upper_inclusive,
                    } => {
                        out.push_str(&format!("entry[{}].expr_kind=range\n", i));
                        out.push_str(&format!("entry[{}].expr_source={}\n", i, source));
                        out.push_str(&format!(
                            "entry[{}].expr.lower_inclusive={}\n",
                            i, lower_inclusive
                        ));
                        if let Some(lower) = lower {
                            out.push_str(&format!("entry[{}].expr.lower.present=true\n", i));
                            out.push_str(&format!(
                                "entry[{}].expr.lower.source={}\n",
                                i, lower.source
                            ));
                            out.push_str(&format!(
                                "entry[{}].expr.lower.component_count={}\n",
                                i,
                                lower.components.len()
                            ));
                        } else {
                            out.push_str(&format!("entry[{}].expr.lower.present=false\n", i));
                        }
                        if let Some(upper) = upper {
                            out.push_str(&format!("entry[{}].expr.upper.present=true\n", i));
                            out.push_str(&format!(
                                "entry[{}].expr.upper.source={}\n",
                                i, upper.source
                            ));
                            out.push_str(&format!(
                                "entry[{}].expr.upper.component_count={}\n",
                                i,
                                upper.components.len()
                            ));
                        } else {
                            out.push_str(&format!("entry[{}].expr.upper.present=false\n", i));
                        }
                        out.push_str(&format!(
                            "entry[{}].expr.upper_inclusive={}\n",
                            i, upper_inclusive
                        ));
                    }
                }
            }
            Err(e) => {
                out.push_str(&format!("entry[{}].parse_error=true\n", i));
                out.push_str(&format!("entry[{}].parse_error_message={}\n", i, e.0));
            }
        }
        out.push_str(&format!("entry[{}].end=true\n", i));
    }
    out
}

pub fn dump_sdk_list(node: &SdkListNode) -> String {
    let mut out = String::new();
    out.push_str(&format!("candidate={}\n", node.candidate));
    out.push_str(&format!("row_count={}\n", node.rows.len()));
    for (idx, row) in node.rows.iter().enumerate() {
        let i = idx + 1;
        out.push_str(&format!("row[{}].version={}\n", i, row.version));
        if let Some(vendor_label) = &row.vendor_label {
            out.push_str(&format!("row[{}].vendor_label.present=true\n", i));
            out.push_str(&format!("row[{}].vendor_label={}\n", i, vendor_label));
        } else {
            out.push_str(&format!("row[{}].vendor_label.present=false\n", i));
        }
        if let Some(dist) = &row.dist {
            out.push_str(&format!("row[{}].dist.present=true\n", i));
            out.push_str(&format!("row[{}].dist={}\n", i, dist));
        } else {
            out.push_str(&format!("row[{}].dist.present=false\n", i));
        }
        if let Some(status) = &row.status {
            out.push_str(&format!("row[{}].status.present=true\n", i));
            out.push_str(&format!("row[{}].status={}\n", i, status));
        } else {
            out.push_str(&format!("row[{}].status.present=false\n", i));
        }
        if let Some(identifier) = &row.identifier {
            out.push_str(&format!("row[{}].identifier.present=true\n", i));
            out.push_str(&format!("row[{}].identifier={}\n", i, identifier));
        } else {
            out.push_str(&format!("row[{}].identifier.present=false\n", i));
        }
        out.push_str(&format!("row[{}].in_use={}\n", i, row.in_use));
    }
    out
}

pub fn self_test(report: impl Fn(&str)) -> Result<()> {
    report("live sdk list");
    let live_sdk_text = run_sdk_list("java")?;
    if !live_sdk_text.contains("Available Java Versions") {
        return Err(err("self-test failed: live sdk list"));
    }
    let live_java_sdk = parse_sdk_list("java", &live_sdk_text);
    if live_java_sdk.rows.is_empty() {
        return Err(err("self-test failed: live java sdk parse"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs, path::PathBuf};

    // ---- helpers ----

    fn parse_expr(s: &str) -> VersionExprNode {
        VersionParser::new(s)
            .parse_version_expr()
            .unwrap_or_else(|e| panic!("parse_expr({s:?}): {e}"))
    }

    fn make_line(s: &str) -> ConfigLineNode {
        ConfigLineParser::new(s, 1)
            .parse_line()
            .unwrap_or_else(|e| panic!("make_line({s:?}): {e}"))
    }

    fn sdk_list(candidate: &str, identifiers: &[&str]) -> SdkListNode {
        let rows = identifiers
            .iter()
            .map(|id| {
                let (version, dist) = if candidate == "java" {
                    match id.rfind('-') {
                        Some(idx) if idx > 0 => {
                            (id[..idx].to_string(), Some(id[idx + 1..].to_string()))
                        }
                        _ => (id.to_string(), None),
                    }
                } else {
                    (id.to_string(), None)
                };
                SdkListRow {
                    candidate: candidate.to_string(),
                    version,
                    vendor_label: None,
                    dist,
                    status: Some("local only".to_string()),
                    identifier: Some(id.to_string()),
                    in_use: false,
                }
            })
            .collect();
        SdkListNode {
            candidate: candidate.to_string(),
            rows,
        }
    }

    fn cmp(a: &str, b: &str) -> i32 {
        let r = Resolver;
        r.compare_versions(a, b)
            .unwrap_or_else(|e| panic!("cmp({a:?}, {b:?}): {e}"))
    }

    fn expr_matches(expr: &str, version: &str) -> bool {
        let r = Resolver;
        r.version_expr_matches(&parse_expr(expr), version)
            .unwrap_or_else(|e| panic!("expr_matches({expr:?}, {version:?}): {e}"))
    }

    // ---- parsing: bare version expansion ----

    #[test]
    fn bare_major_expands_to_major_range() {
        match parse_expr("21") {
            VersionExprNode::Range {
                lower_inclusive,
                lower,
                upper,
                upper_inclusive,
                ..
            } => {
                assert!(lower_inclusive);
                assert!(!upper_inclusive);
                assert_eq!(lower.unwrap().source, "21");
                assert_eq!(upper.unwrap().source, "22");
            }
            _ => panic!("expected range"),
        }
    }

    #[test]
    fn bare_minor_expands_to_minor_range() {
        match parse_expr("3.9") {
            VersionExprNode::Range {
                lower_inclusive,
                lower,
                upper,
                upper_inclusive,
                ..
            } => {
                assert!(lower_inclusive);
                assert!(!upper_inclusive);
                assert_eq!(lower.unwrap().source, "3.9");
                assert_eq!(upper.unwrap().source, "3.10");
            }
            _ => panic!("expected range"),
        }
    }

    #[test]
    fn bare_three_segment_is_exact() {
        assert!(matches!(parse_expr("8.7.0"), VersionExprNode::Exact { .. }));
    }

    #[test]
    fn bare_four_segment_is_exact() {
        assert!(matches!(parse_expr("4.10.1.3"), VersionExprNode::Exact { .. }));
    }

    #[test]
    fn bare_mixed_version_is_exact() {
        assert!(matches!(parse_expr("26.ea.35"), VersionExprNode::Exact { .. }));
    }

    #[test]
    fn bare_release_alias_version_is_exact() {
        assert!(matches!(
            parse_expr("2.16.0.Final"),
            VersionExprNode::Exact { .. }
        ));
    }

    #[test]
    fn bare_underscore_version_is_exact() {
        assert!(matches!(
            parse_expr("5.23.0.0_2"),
            VersionExprNode::Exact { .. }
        ));
    }

    // ---- parsing: explicit ranges ----

    #[test]
    fn explicit_range_inclusive_lower_exclusive_upper() {
        match parse_expr("[21,22)") {
            VersionExprNode::Range {
                lower_inclusive,
                lower,
                upper,
                upper_inclusive,
                ..
            } => {
                assert!(lower_inclusive);
                assert!(!upper_inclusive);
                assert_eq!(lower.unwrap().source, "21");
                assert_eq!(upper.unwrap().source, "22");
            }
            _ => panic!("expected range"),
        }
    }

    #[test]
    fn explicit_range_inclusive_both_bounds() {
        match parse_expr("[3.9,4]") {
            VersionExprNode::Range {
                lower_inclusive,
                lower,
                upper,
                upper_inclusive,
                ..
            } => {
                assert!(lower_inclusive);
                assert!(upper_inclusive);
                assert_eq!(lower.unwrap().source, "3.9");
                assert_eq!(upper.unwrap().source, "4");
            }
            _ => panic!("expected range"),
        }
    }

    #[test]
    fn explicit_range_exclusive_lower_inclusive_upper() {
        match parse_expr("(21,22]") {
            VersionExprNode::Range {
                lower_inclusive,
                lower,
                upper,
                upper_inclusive,
                ..
            } => {
                assert!(!lower_inclusive);
                assert!(upper_inclusive);
                assert_eq!(lower.unwrap().source, "21");
                assert_eq!(upper.unwrap().source, "22");
            }
            _ => panic!("expected range"),
        }
    }

    #[test]
    fn explicit_range_no_upper_bound() {
        match parse_expr("[21,)") {
            VersionExprNode::Range { lower, upper, .. } => {
                assert!(lower.is_some());
                assert!(upper.is_none());
            }
            _ => panic!("expected range"),
        }
    }

    #[test]
    fn explicit_range_no_lower_bound() {
        match parse_expr("(,22)") {
            VersionExprNode::Range { lower, upper, .. } => {
                assert!(lower.is_none());
                assert!(upper.is_some());
            }
            _ => panic!("expected range"),
        }
    }

    #[test]
    fn bracketed_single_value_is_exact() {
        assert!(matches!(
            parse_expr("[21.0.5]"),
            VersionExprNode::Exact { .. }
        ));
    }

    // ---- parsing: config lines ----

    #[test]
    fn config_line_no_vendor() {
        let line = make_line("java = 21");
        assert_eq!(line.candidate, "java");
        assert!(line.vendor.is_none());
    }

    #[test]
    fn config_line_with_vendor() {
        let line = make_line("java = 21 tem");
        assert_eq!(line.candidate, "java");
        assert_eq!(line.vendor.as_deref(), Some("tem"));
    }

    #[test]
    fn config_line_range_expr() {
        let line = make_line("maven = [3.9,4)");
        assert_eq!(line.candidate, "maven");
        assert!(matches!(line.expr, VersionExprNode::Range { .. }));
    }

    #[test]
    fn config_line_whitespace_trimmed() {
        let line = make_line("  java = 21  ");
        assert_eq!(line.candidate, "java");
    }

    #[test]
    fn config_line_missing_equals_is_error() {
        assert!(ConfigLineParser::new("java 21", 1).parse_line().is_err());
    }

    // ---- parsing: document ----

    #[test]
    fn document_skips_blank_lines_and_comments() {
        let doc = parse_document("# comment\n\nmaven = [3.9.14]\njava 21\n");
        assert_eq!(doc.entries.len(), 2);
        assert_eq!(doc.entries[0].source, "maven = [3.9.14]");
        assert_eq!(doc.entries[1].source, "java 21");
    }

    #[test]
    fn document_line_numbers_are_correct() {
        let doc = parse_document("# comment\n\nmaven = [3.9.14]\njava 21\n");
        assert_eq!(doc.entries[0].line_number, 3);
        assert_eq!(doc.entries[1].line_number, 4);
    }

    // ---- parsing: sdk list (generic grid) ----

    #[test]
    fn generic_sdk_list_parses_status_flags() {
        let fixture = "================================================================================\nAvailable Demo Versions\n================================================================================\n > + 1.2.3               * 1.2.2             + 1.2.1                            \n\n================================================================================\n+ - local version\n* - installed\n> - currently in use\n================================================================================\n";
        let node = parse_sdk_list("demo", fixture);
        assert_eq!(node.rows.len(), 3);
        assert_eq!(node.rows[0].version, "1.2.3");
        assert_eq!(node.rows[0].status.as_deref(), Some("current local only"));
        assert_eq!(node.rows[1].status.as_deref(), Some("installed"));
        assert_eq!(node.rows[2].status.as_deref(), Some("local only"));
    }

    // ---- parsing: sdk list (java pipe table) ----

    #[test]
    fn java_sdk_list_parses_vendor_dist_and_in_use() {
        let fixture = "================================================================================\nAvailable Java Versions for Test Platform\n================================================================================\n Vendor        | Use | Version      | Dist    | Status     | Identifier\n--------------------------------------------------------------------------------\n GraalVM CE    | >>> | 25.0.1       | graalce | local only | 25.0.1-graalce\n               |     | 24.0.2       | graalce | installed  | 24.0.2-graalce\n";
        let node = parse_sdk_list("java", fixture);
        assert_eq!(node.rows.len(), 2);
        assert_eq!(node.rows[0].vendor_label.as_deref(), Some("GraalVM CE"));
        assert_eq!(node.rows[0].dist.as_deref(), Some("graalce"));
        assert!(node.rows[0].in_use);
        assert_eq!(
            node.rows[0].identifier.as_deref(),
            Some("25.0.1-graalce")
        );
        // Vendor carries over to continuation row
        assert_eq!(node.rows[1].vendor_label.as_deref(), Some("GraalVM CE"));
        assert_eq!(node.rows[1].dist.as_deref(), Some("graalce"));
        assert!(!node.rows[1].in_use);
        assert_eq!(
            node.rows[1].identifier.as_deref(),
            Some("24.0.2-graalce")
        );
    }

    // ---- version comparison: numerics ----

    #[test]
    fn numeric_comparison_is_not_lexicographic() {
        assert!(cmp("9", "10") < 0, "9 should be less than 10");
        assert!(cmp("3.9.9", "3.9.14") < 0, "3.9.9 should be less than 3.9.14");
    }

    // ---- version comparison: pre-release ordering ----

    #[test]
    fn prerelease_sorts_before_plain_release() {
        assert!(cmp("26.ea.35", "26") < 0);
        assert!(cmp("1.0.alpha", "1.0") < 0);
        assert!(cmp("1.0.snapshot", "1.0") < 0);
    }

    #[test]
    fn prerelease_order_within_qualifiers() {
        // alpha < beta < milestone < rc < ea < preview < snapshot < release
        assert!(cmp("1.0.alpha", "1.0.beta") < 0);
        assert!(cmp("1.0.beta", "1.0.milestone") < 0);
        assert!(cmp("1.0.milestone", "1.0.rc") < 0);
        assert!(cmp("1.0.rc", "1.0.ea") < 0);
        assert!(cmp("1.0.ea", "1.0.preview") < 0);
        assert!(cmp("1.0.preview", "1.0.snapshot") < 0);
    }

    #[test]
    fn prerelease_aliases_have_same_rank() {
        // a == alpha, b == beta, m == milestone, cr == rc
        // All four aliases sort with the same rank as their canonical form
        assert!(cmp("1.0.a", "1.0.beta") < 0);
        assert!(cmp("1.0.alpha", "1.0.beta") < 0);
        assert!(cmp("1.0.b", "1.0.milestone") < 0);
        assert!(cmp("1.0.beta", "1.0.milestone") < 0);
        assert!(cmp("1.0.m", "1.0.rc") < 0);
        assert!(cmp("1.0.milestone", "1.0.rc") < 0);
        assert!(cmp("1.0.cr", "1.0.ea") < 0);
        assert!(cmp("1.0.rc", "1.0.ea") < 0);
    }

    // ---- version comparison: release aliases ----

    #[test]
    fn release_alias_skipped_in_unit_comparison_but_ordered_by_string() {
        // Units are equal (Final/ga/release are skipped), but string tiebreaker
        // places the alias-suffixed form after the plain version.
        assert!(cmp("2.16.0.Final", "2.16.0") > 0);
        assert!(cmp("2.16.0.ga", "2.16.0") > 0);
        assert!(cmp("2.16.0.release", "2.16.0") > 0);
    }

    // ---- version comparison: variants ----

    #[test]
    fn variant_qualifiers_sort_after_plain_release() {
        assert!(cmp("21.0.10", "21.0.10.fx") < 0);
        assert!(cmp("21.0.10", "21.0.10.crac") < 0);
    }

    // ---- version comparison: post-release ----

    #[test]
    fn post_release_suffix_sorts_after_base() {
        assert!(cmp("25.0.2", "25.0.2.r25") < 0);
        assert!(cmp("5.0.0_1", "5.0.0_2") < 0);
        assert!(cmp("1.0.1-1", "1.0.1-2") < 0);
    }

    // ---- version comparison: numeric pre-release suffixes ----

    #[test]
    fn prerelease_with_numeric_suffix_ordered_numerically() {
        assert!(cmp("1.0.rc1", "1.0.rc2") < 0);
        assert!(cmp("26.ea.13", "26.ea.35") < 0);
        assert!(cmp("1.0.M1", "1.0.M2") < 0);
    }

    // ---- version comparison: exhaustion rules ----

    #[test]
    fn trailing_zero_is_greater_than_absent() {
        // 9.0 has no third component; 9.0.0 does — the extra zero is not elided
        assert!(cmp("9.0", "9.0.0") < 0);
    }

    #[test]
    fn plain_release_beats_prerelease_when_one_side_exhausts() {
        // 9.0 exhausts while 9.0.rc1 still has a pre-release token: 9.0 wins
        assert!(cmp("9.0", "9.0.rc1") > 0);
    }

    // ---- range membership ----

    #[test]
    fn stable_versions_match_plain_range() {
        assert!(expr_matches("[26,)", "26"));
        assert!(expr_matches("[26,)", "27"));
        assert!(expr_matches("[26,)", "27.0.1"));
    }

    #[test]
    fn prerelease_excluded_from_plain_range() {
        assert!(!expr_matches("[26,)", "26.ea.35"));
        assert!(!expr_matches("[26,)", "27.ea.14"));
    }

    #[test]
    fn prerelease_included_when_lower_bound_opts_in() {
        assert!(expr_matches("[26.ea,)", "26.ea.13"));
        assert!(expr_matches("[26.ea,)", "26.ea.35"));
        assert!(expr_matches("[26.ea,)", "27")); // stable always passes
    }

    #[test]
    fn prerelease_opt_in_is_base_specific() {
        // [26.ea,) opts in for 26.ea.* only, not for 27.ea.*
        assert!(!expr_matches("[26.ea,)", "27.ea.14"));
    }

    #[test]
    fn upper_bound_prerelease_opts_in_for_same_base() {
        assert!(expr_matches("[26.1,27.ea]", "27.ea"));
        assert!(!expr_matches("[26.1,27.ea]", "27.ea.14")); // exceeds upper bound
        assert!(!expr_matches("[26.1,27.ea]", "26.ea.35")); // different base (26 ≠ 27)
    }

    #[test]
    fn variant_qualifiers_included_in_containing_range() {
        assert!(expr_matches("[21.0.10,21.0.11)", "21.0.10"));
        assert!(expr_matches("[21.0.10,21.0.11)", "21.0.10.fx"));
        assert!(expr_matches("[21.0.10,21.0.11)", "21.0.10.crac"));
    }

    #[test]
    fn exact_expression_matches_release_aliases() {
        assert!(expr_matches("[2.16.0]", "2.16.0.Final"));
        assert!(expr_matches("[2.16.0]", "2.16.0"));
        assert!(expr_matches("[2.16.0]", "2.16.0.ga"));
    }

    #[test]
    fn exclusive_bounds_exclude_bound_values() {
        assert!(!expr_matches("[21,22)", "22")); // exclusive upper
        assert!(!expr_matches("(21,22]", "21")); // exclusive lower
        assert!(expr_matches("[21,22)", "21")); // inclusive lower
        assert!(expr_matches("(21,22]", "22")); // inclusive upper
    }

    // ---- vendor matching ----

    #[test]
    fn no_vendor_on_line_matches_any_dist() {
        let resolver = Resolver;
        let line = make_line("java = [21.0.2] ");
        let list = sdk_list("java", &["21.0.2-tem", "21.0.2-graalce"]);
        // Both rows match because no vendor is specified
        let count = list
            .rows
            .iter()
            .filter(|row| resolver.vendor_matches(&line, row))
            .count();
        assert_eq!(count, 2);
    }

    #[test]
    fn vendor_requires_exact_dist_match() {
        let resolver = Resolver;
        let line = make_line("java = [21] graalce");
        let list = sdk_list("java", &["21.0.2-tem", "21.0.2-graalce"]);
        let matched: Vec<_> = list
            .rows
            .iter()
            .filter(|row| resolver.vendor_matches(&line, row))
            .collect();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].dist.as_deref(), Some("graalce"));
    }

    #[test]
    fn vendor_match_is_case_sensitive() {
        let resolver = Resolver;
        let line = make_line("java = [21] GraalCE");
        let list = sdk_list("java", &["21.0.2-graalce"]);
        // "GraalCE" != "graalce"
        let any_match = list
            .rows
            .iter()
            .any(|row| resolver.vendor_matches(&line, row));
        assert!(!any_match);
    }

    // ---- resolution ----

    #[test]
    fn resolves_highest_matching_version() {
        let resolver = Resolver;
        let line = make_line("demo = [1,2)");
        let list = sdk_list("demo", &["1.0.0", "1.0.2", "1.0.1"]);
        let result = resolver.resolve_line(&line, &list).unwrap();
        assert_eq!(result.version, "1.0.2");
    }

    #[test]
    fn vendor_filter_selects_matching_dist() {
        let resolver = Resolver;
        let line = make_line("java = [21,22) tem");
        let list = sdk_list("java", &["21.0.2-graalce", "21.0.2-tem"]);
        let result = resolver.resolve_line(&line, &list).unwrap();
        assert_eq!(result.dist.as_deref(), Some("tem"));
    }

    #[test]
    fn no_matching_version_returns_error() {
        let resolver = Resolver;
        let line = make_line("demo = [2,3)");
        let list = sdk_list("demo", &["1.0.0"]);
        assert!(resolver.resolve_line(&line, &list).is_err());
    }

    #[test]
    fn empty_sdk_list_returns_error() {
        let resolver = Resolver;
        let line = make_line("demo = [1,2)");
        let list = sdk_list("demo", &[]);
        assert!(resolver.resolve_line(&line, &list).is_err());
    }

    #[test]
    fn identifier_used_as_resolution_target() {
        let resolver = Resolver;
        let line = make_line("java = [21.0.2] tem");
        let list = sdk_list("java", &["21.0.2-tem"]);
        let result = resolver.resolve_line(&line, &list).unwrap();
        // target should be the identifier "21.0.2-tem", not just the version "21.0.2"
        assert_eq!(result.target, "21.0.2-tem");
        assert_eq!(result.version, "21.0.2");
    }

    #[test]
    fn continues_after_malformed_document_entries() {
        let doc = parse_document("demo = [1.2.3]\njava 21\n");
        let resolver = Resolver;
        let list = sdk_list("demo", &["1.2.3"]);
        let mut successes = 0;
        let mut errors = 0;
        for entry in doc.entries {
            match ConfigLineParser::new(&entry.source, entry.line_number).parse_line() {
                Ok(config) if config.candidate == "demo" => {
                    match resolver.resolve_line(&config, &list) {
                        Ok(_) => successes += 1,
                        Err(_) => errors += 1,
                    }
                }
                Ok(_) => errors += 1, // java 21 has no sdk list available
                Err(_) => errors += 1,
            }
        }
        assert_eq!(successes, 1);
        assert_eq!(errors, 1);
    }

    // ---- project discovery ----

    #[test]
    fn finds_sdkvers_file_in_ancestor_directory() {
        let temp_root = env::temp_dir().join(format!("sdkvers-test-{}", std::process::id()));
        let nested = temp_root.join("a").join("b");
        fs::create_dir_all(&nested).unwrap();
        let file_path = temp_root.join(".sdkvers");
        fs::write(&file_path, "java = 21\n").unwrap();
        let result = find_sdkvers_path(nested.to_string_lossy().as_ref());
        // Canonicalize while files still exist, then clean up.
        let found = result.map(|p| {
            fs::canonicalize(&p).unwrap_or_else(|_| PathBuf::from(&p))
        });
        let expected = fs::canonicalize(&file_path).unwrap_or(file_path);
        let _ = fs::remove_dir_all(&temp_root);
        assert_eq!(found.unwrap(), expected);
    }

    #[test]
    fn returns_error_when_no_sdkvers_file_exists() {
        let temp_root = env::temp_dir().join(format!("sdkvers-test-nofile-{}", std::process::id()));
        let nested = temp_root.join("a");
        fs::create_dir_all(&nested).unwrap();
        let result = find_sdkvers_path(nested.to_string_lossy().as_ref());
        let _ = fs::remove_dir_all(&temp_root);
        assert!(result.is_err());
    }
}

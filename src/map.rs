use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use ignore::WalkBuilder;
use thiserror::Error;

use crate::{
    config::{CheckSeverity, Config, Kind, Tier},
    report::{Finding, Severity},
};

const README_FILE: &str = "README.md";
const MAP_HEADING: &str = "## Repository map";
const BEGIN_MARKER: &str = "<!-- MAP:BEGIN";
const END_MARKER: &str = "<!-- MAP:END";
const COMPONENT_ZONES: &[&str] = &["apps", "services", "crates", "packages", "tools"];
const STANDARD_ROOT_FILES: &[&str] = &[
    "README.md",
    "LICENSE",
    "LICENSE.md",
    "AGENTS.md",
    "CLAUDE.md",
    "SECURITY.md",
    "CHANGELOG.md",
    "CONTRIBUTING.md",
];

#[derive(Debug, Error)]
pub enum MapError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to write {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to walk repository: {0}")]
    Walk(String),
    #[error("README.md has malformed repository-map markers: {0}")]
    MalformedReadme(String),
    #[error("README.md does not contain an identity block")]
    MissingIdentity,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MapRow {
    path: String,
    directory: bool,
    description: Option<String>,
}

#[derive(Debug)]
pub struct MapDocument {
    block: String,
    rows: Vec<MapRow>,
}

impl MapDocument {
    pub fn build(root: &Path, config: &Config) -> Result<Self, MapError> {
        let paths = discover_paths(root, config)?;
        let mut rows = Vec::new();

        for path in paths {
            rows.push(MapRow {
                description: describe_path(root, &path, config),
                directory: true,
                path,
            });
        }

        for path in config
            .map
            .files
            .keys()
            .filter(|path| !STANDARD_ROOT_FILES.contains(&path.as_str()))
        {
            rows.push(MapRow {
                description: Some(config.map.files[path].clone()),
                directory: false,
                path: path.clone(),
            });
        }

        rows.sort_by(|left, right| left.path.cmp(&right.path));
        let block = render_block(
            config.repo.as_ref().map_or(Tier::Leaf, |repo| repo.tier),
            &rows,
        );
        Ok(Self { block, rows })
    }

    pub fn block(&self) -> &str {
        &self.block
    }

    pub fn write(&self, root: &Path) -> Result<(), MapError> {
        let path = root.join(README_FILE);
        let source = fs::read_to_string(&path).map_err(|source| MapError::Read {
            path: path.clone(),
            source,
        })?;
        let mut lines: Vec<String> = source.split('\n').map(ToOwned::to_owned).collect();
        let block_lines: Vec<String> = self
            .block
            .trim_end_matches('\n')
            .split('\n')
            .map(ToOwned::to_owned)
            .collect();

        let markers = locate_markers(&lines)?;
        if let Some((begin, end)) = markers {
            lines[..begin]
                .iter()
                .rposition(|line| line.trim() == MAP_HEADING)
                .ok_or_else(|| {
                    MapError::MalformedReadme("markers have no map heading".to_owned())
                })?;
            let generated_lines: Vec<&str> = self.block.lines().collect();
            let generated_begin = generated_lines
                .iter()
                .position(|line| is_begin_marker(line))
                .ok_or_else(|| {
                    MapError::MalformedReadme("generated block has no BEGIN marker".to_owned())
                })?;
            lines.splice(
                begin..=end,
                generated_lines[generated_begin..]
                    .iter()
                    .map(|line| (*line).to_owned()),
            );
        } else {
            if lines.iter().any(|line| line.trim() == MAP_HEADING) {
                return Err(MapError::MalformedReadme(
                    "map heading exists without one marker pair".to_owned(),
                ));
            }
            let identity_end = identity_end(&lines).ok_or(MapError::MissingIdentity)?;
            lines.splice(identity_end + 1..identity_end + 1, block_lines);
        }

        let mut output = lines.join("\n");
        if !output.ends_with('\n') {
            output.push('\n');
        }
        fs::write(&path, output).map_err(|source| MapError::Write { path, source })
    }
}

pub fn check(root: &Path, config: &Config) -> Result<Vec<Finding>, MapError> {
    let Some(repo) = config.repo.as_ref() else {
        return Ok(Vec::new());
    };
    if repo.research || repo.kind == Kind::OutOfScope {
        return Ok(Vec::new());
    }

    let default_severity = if repo.tier == Tier::Leaf {
        CheckSeverity::Warn
    } else {
        CheckSeverity::Error
    };
    let Some(severity) = effective_severity(config.severity("map.gate", default_severity)) else {
        return Ok(Vec::new());
    };

    let document = MapDocument::build(root, config)?;
    let readme_path = root.join(README_FILE);
    if !readme_path.exists() {
        if !repo.tier.requires_map() {
            return Ok(Vec::new());
        }
        return Ok(vec![finding(
            severity,
            "README.md is required for the generated repository map",
            Some(README_FILE.to_owned()),
        )]);
    }
    let source = fs::read_to_string(&readme_path).map_err(|source| MapError::Read {
        path: readme_path,
        source,
    })?;
    let lines: Vec<&str> = source.split('\n').collect();
    let owned_lines: Vec<String> = lines.iter().map(|line| (*line).to_owned()).collect();
    let begin_count = lines.iter().filter(|line| is_begin_marker(line)).count();
    let end_count = lines.iter().filter(|line| is_end_marker(line)).count();
    if begin_count == 0 && end_count == 0 && !repo.tier.requires_map() {
        return Ok(Vec::new());
    }

    let mut findings = Vec::new();
    if begin_count == 0 && end_count == 0 {
        findings.push(finding(
            severity,
            "repository map section is missing; run `repolint map --write`",
            Some(README_FILE.to_owned()),
        ));
        return Ok(findings);
    }
    if begin_count != 1 || end_count != 1 {
        findings.push(finding(
            severity,
            "repository map must contain exactly one balanced MAP:BEGIN/MAP:END pair",
            Some(README_FILE.to_owned()),
        ));
        return Ok(findings);
    }

    let (begin, end) = locate_markers(&owned_lines)?.ok_or_else(|| {
        MapError::MalformedReadme("marker pair disappeared during validation".to_owned())
    })?;
    let heading = lines[..begin]
        .iter()
        .rposition(|line| line.trim() == MAP_HEADING);
    let Some(heading) = heading else {
        findings.push(finding(
            severity,
            "repository map markers must be inside a ## Repository map section",
            Some(README_FILE.to_owned()),
        ));
        return Ok(findings);
    };

    if identity_end(&owned_lines)
        .is_none_or(|identity| next_nonempty(&lines, identity + 1) != Some(heading))
    {
        findings.push(finding(
            severity,
            "repository map must be directly after the README identity block",
            Some(README_FILE.to_owned()),
        ));
    }

    let actual = format!("{}\n", lines[begin..=end].join("\n"));
    let expected = document
        .block
        .lines()
        .skip_while(|line| !is_begin_marker(line))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    if actual != expected {
        findings.push(finding(
            severity,
            "repository map drifted from its sources; run `repolint map --write`",
            Some(README_FILE.to_owned()),
        ));
    }

    for path in config.map.dirs.keys() {
        if !root.join(path).is_dir() {
            findings.push(finding(
                severity,
                "map.dirs source points at a missing directory",
                Some(path.clone()),
            ));
        }
    }

    let actual_rows = parse_rows(&lines[begin..=end]);
    let mut actual_paths = BTreeMap::<String, usize>::new();
    for row in &actual_rows {
        *actual_paths.entry(row.path.clone()).or_default() += 1;
        let target = root.join(&row.path);
        if !target.exists() || (row.directory && !target.is_dir()) {
            findings.push(finding(
                severity,
                "repository map row points at a missing path",
                Some(row.path.clone()),
            ));
        }
        if row.description.is_none() {
            findings.push(finding(
                severity,
                "repository map row is missing a one-line description",
                Some(row.path.clone()),
            ));
        }
    }

    for (path, count) in actual_paths {
        if count > 1 {
            findings.push(finding(
                severity,
                "repository map path is listed more than once",
                Some(path),
            ));
        }
    }

    let expected_paths: BTreeSet<&str> =
        document.rows.iter().map(|row| row.path.as_str()).collect();
    let actual_paths: BTreeSet<&str> = actual_rows.iter().map(|row| row.path.as_str()).collect();
    for path in expected_paths.difference(&actual_paths) {
        let required =
            repo.tier.requires_map() || document.rows.iter().any(|row| row.path == *path);
        if required {
            findings.push(finding(
                severity,
                "repository map is missing a source path",
                Some((*path).to_owned()),
            ));
        }
    }
    for path in actual_paths.difference(&expected_paths) {
        findings.push(finding(
            severity,
            "repository map contains a row not produced by its sources",
            Some((*path).to_owned()),
        ));
    }

    if repo.tier.requires_map() {
        for row in &document.rows {
            if row.description.is_none() {
                findings.push(finding(
                    severity,
                    "workspace map source has no description; add README prose or [map.dirs]",
                    Some(row.path.clone()),
                ));
            }
        }
    }

    Ok(findings)
}

fn effective_severity(severity: CheckSeverity) -> Option<Severity> {
    match severity {
        CheckSeverity::Off => None,
        CheckSeverity::Warn => Some(Severity::Warn),
        CheckSeverity::Error => Some(Severity::Error),
    }
}

fn finding(severity: Severity, message: &str, path: Option<String>) -> Finding {
    Finding::new("map.gate", severity, message, path)
}

fn discover_paths(root: &Path, config: &Config) -> Result<Vec<String>, MapError> {
    let mut paths = BTreeSet::new();
    let mut walker = WalkBuilder::new(root);
    walker
        .hidden(true)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .ignore(true)
        .max_depth(Some(2))
        .parents(false);

    for entry in walker.build() {
        let entry = entry.map_err(|error| MapError::Walk(error.to_string()))?;
        if !entry
            .file_type()
            .is_some_and(|file_type| file_type.is_dir())
        {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        let path = slash_path(relative);
        if path.is_empty() || is_generated(&path, config) {
            continue;
        }
        let components: Vec<&str> = path.split('/').collect();
        if components.len() == 1
            || (components.len() == 2
                && COMPONENT_ZONES.contains(&components[0])
                && (has_component_marker(entry.path()) || config.map.dirs.contains_key(&path)))
        {
            paths.insert(path);
        }
    }
    Ok(paths.into_iter().collect())
}

fn is_generated(path: &str, config: &Config) -> bool {
    config.generated.entry.iter().any(|entry| {
        entry.outputs.iter().any(|output| {
            let output = output.trim_end_matches('/');
            if output.is_empty() {
                return false;
            }
            if let Some(prefix) = output.strip_suffix("/**") {
                return path == prefix || path.starts_with(&format!("{prefix}/"));
            }
            if output.contains('*') {
                simple_glob_match(output, path)
            } else {
                path == output || path.starts_with(&format!("{output}/"))
            }
        })
    })
}

fn simple_glob_match(pattern: &str, path: &str) -> bool {
    let mut remaining = path;
    for part in pattern.split('*') {
        if part.is_empty() {
            continue;
        }
        let Some(index) = remaining.find(part) else {
            return false;
        };
        remaining = &remaining[index + part.len()..];
    }
    true
}

fn has_component_marker(path: &Path) -> bool {
    ["Cargo.toml", "package.json", "Package.swift", "README.md"]
        .iter()
        .any(|file| path.join(file).is_file())
}

fn describe_path(root: &Path, path: &str, config: &Config) -> Option<String> {
    if let Some(description) = config.map.dirs.get(path) {
        return Some(description.clone());
    }
    let full_path = root.join(path);
    manifest_description(&full_path).or_else(|| readme_description(&full_path.join(README_FILE)))
}

fn manifest_description(path: &Path) -> Option<String> {
    let cargo = path.join("Cargo.toml");
    if cargo.is_file()
        && let Ok(source) = fs::read_to_string(cargo)
        && let Ok(value) = source.parse::<toml::Value>()
        && let Some(description) = value
            .get("package")
            .and_then(|package| package.get("description"))
            .and_then(toml::Value::as_str)
    {
        return Some(description.to_owned());
    }

    let package = path.join("package.json");
    if package.is_file()
        && let Ok(source) = fs::read_to_string(package)
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&source)
        && let Some(description) = value.get("description").and_then(serde_json::Value::as_str)
    {
        return Some(description.to_owned());
    }
    None
}

fn readme_description(path: &Path) -> Option<String> {
    let source = fs::read_to_string(path).ok()?;
    let mut in_comment = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.contains("<!--") {
            in_comment = true;
        }
        if in_comment {
            if trimmed.contains("-->") {
                in_comment = false;
            }
            continue;
        }
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.starts_with("[![")
            || trimmed.starts_with("![")
            || trimmed.starts_with("<")
        {
            continue;
        }
        return Some(first_sentence(trimmed));
    }
    None
}

fn first_sentence(line: &str) -> String {
    for (index, character) in line.char_indices() {
        if matches!(character, '.' | '!' | '?')
            && line[index + character.len_utf8()..]
                .chars()
                .next()
                .is_none_or(char::is_whitespace)
        {
            return line[..index + character.len_utf8()].to_owned();
        }
    }
    line.to_owned()
}

fn render_block(tier: Tier, rows: &[MapRow]) -> String {
    let mut output = String::from("## Repository map\n\n<!-- MAP:BEGIN -->\n");
    output.push_str(&format!(
        "> {tier} tier · drift-checked in `mise run fleet:check` · details live in each component's README\n\n"
    ));
    output.push_str("| Path | What lives here |\n|---|---|\n");

    let mut root_files = false;
    for row in rows {
        if !row.directory {
            root_files = true;
            continue;
        }
        render_row(&mut output, row);
    }
    if root_files {
        output.push_str("\n### Root files worth knowing\n\n| Path | Purpose |\n|---|---|\n");
        for row in rows.iter().filter(|row| !row.directory) {
            render_row(&mut output, row);
        }
    }
    output.push_str("<!-- MAP:END -->\n");
    output
}

fn render_row(output: &mut String, row: &MapRow) {
    let target = if row.directory {
        format!("{}/", row.path)
    } else {
        row.path.clone()
    };
    let description = row.description.as_deref().unwrap_or("MISSING");
    output.push_str(&format!(
        "| [`{target}`]({target}) | {} |\n",
        description.replace('|', "\\|")
    ));
}

fn parse_rows(lines: &[&str]) -> Vec<MapRow> {
    lines.iter().filter_map(|line| parse_row(line)).collect()
}

fn parse_row(line: &str) -> Option<MapRow> {
    let fields: Vec<&str> = line.split('|').collect();
    if fields.len() < 3 {
        return None;
    }
    let link = fields[1].trim();
    let link_start = link.find("](")?;
    let target_start = link_start + 2;
    let target_end = link[target_start..].find(')')? + target_start;
    let target = link[target_start..target_end].trim();
    if target.is_empty() || target.starts_with('#') || target.starts_with("http") {
        return None;
    }
    let directory = target.ends_with('/');
    let path = target.trim_end_matches('/').to_owned();
    let description = (!fields[2].trim().is_empty() && fields[2].trim() != "MISSING")
        .then(|| fields[2].trim().replace("\\|", "|"));
    Some(MapRow {
        path,
        directory,
        description,
    })
}

fn locate_markers(lines: &[String]) -> Result<Option<(usize, usize)>, MapError> {
    let begins: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| is_begin_marker(line).then_some(index))
        .collect();
    let ends: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| is_end_marker(line).then_some(index))
        .collect();
    if begins.is_empty() && ends.is_empty() {
        return Ok(None);
    }
    if begins.len() != 1 || ends.len() != 1 || begins[0] >= ends[0] {
        return Err(MapError::MalformedReadme(
            "expected one BEGIN before one END".to_owned(),
        ));
    }
    Ok(Some((begins[0], ends[0])))
}

fn is_begin_marker(line: &str) -> bool {
    line.trim_start().starts_with(BEGIN_MARKER)
}

fn is_end_marker(line: &str) -> bool {
    line.trim_start().starts_with(END_MARKER)
}

fn identity_end(lines: &[String]) -> Option<usize> {
    let title = lines.iter().position(|line| {
        let trimmed = line.trim();
        trimmed.starts_with("# ") && !trimmed.starts_with("## ")
    })?;
    lines
        .iter()
        .enumerate()
        .skip(title + 1)
        .find_map(|(index, line)| {
            let trimmed = line.trim();
            (!trimmed.is_empty()
                && !trimmed.starts_with('#')
                && !trimmed.starts_with("<!--")
                && !trimmed.starts_with("[![")
                && !trimmed.starts_with("!["))
            .then_some(index)
        })
}

fn next_nonempty(lines: &[&str], start: usize) -> Option<usize> {
    lines
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(index, line)| (!line.trim().is_empty()).then_some(index))
}

fn slash_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => Some(value.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::readme_description;
    use std::fs;

    #[test]
    fn headings_and_badges_do_not_become_descriptions() {
        let path = std::env::temp_dir().join(format!(
            "repolint-description-{}-{}.md",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        fs::write(
            &path,
            "# Component\n\n[![CI](badge)](ci)\n\n## Details\n\nUseful component. More text.",
        )
        .expect("test fixture write");
        assert_eq!(
            readme_description(&path).as_deref(),
            Some("Useful component.")
        );
        fs::remove_file(path).expect("test fixture cleanup");
    }
}

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use thiserror::Error;

const CONFIG_FILE: &str = "repolint.toml";

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub repo: Option<RepoConfig>,
    #[serde(default)]
    pub map: MapConfig,
    #[serde(default)]
    pub generated: GeneratedConfig,
    #[serde(default)]
    pub checks: BTreeMap<String, CheckSeverity>,
    #[serde(default)]
    pub ignore: Vec<IgnoreRule>,
    #[serde(default)]
    pub deferred: DeferredConfig,
}

#[derive(Debug, Deserialize)]
pub struct RepoConfig {
    pub tier: Tier,
    pub kind: Kind,
    pub visibility: Visibility,
    pub research: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Leaf,
    Workspace,
    Polyglot,
}

impl Tier {
    pub fn requires_map(self) -> bool {
        matches!(self, Self::Workspace | Self::Polyglot)
    }
}

impl std::fmt::Display for Tier {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Leaf => formatter.write_str("Leaf"),
            Self::Workspace => formatter.write_str("Workspace"),
            Self::Polyglot => formatter.write_str("Polyglot"),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    App,
    Iac,
    Dist,
    #[serde(rename = "ci-producer")]
    CiProducer,
    #[serde(rename = "out-of-scope")]
    OutOfScope,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    Private,
    Internal,
}

#[derive(Debug, Default, Deserialize)]
pub struct MapConfig {
    #[serde(default)]
    pub dirs: BTreeMap<String, String>,
    #[serde(default)]
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct GeneratedConfig {
    #[serde(default)]
    pub entry: Vec<GeneratedEntry>,
}

#[derive(Debug, Deserialize)]
pub struct GeneratedEntry {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub outputs: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct IgnoreRule {
    pub check: String,
    pub paths: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct DeferredConfig {
    #[serde(default)]
    pub item: Vec<DeferredItem>,
}

#[derive(Debug, Deserialize)]
pub struct DeferredItem {
    pub item: String,
    pub registered: String,
    pub last_deferred: String,
    pub reason: String,
    pub effort: String,
    pub trigger: String,
    pub blocking_gate: Option<String>,
    #[serde(default)]
    pub steps: Vec<DeferredStep>,
}

#[derive(Debug, Deserialize)]
pub struct DeferredStep {
    pub step: String,
    pub date: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CheckSeverity {
    Off,
    Warn,
    Error,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("invalid {path}: {message}")]
    Invalid { path: PathBuf, message: String },
}

impl Config {
    pub fn load(root: &Path) -> Result<Self, ConfigError> {
        let path = root.join(CONFIG_FILE);
        if !path.exists() {
            return Ok(Self::default());
        }

        let source = fs::read_to_string(&path).map_err(|source| ConfigError::Read {
            path: path.clone(),
            source,
        })?;
        let config: Self = toml::from_str(&source).map_err(|source| ConfigError::Parse {
            path: path.clone(),
            source,
        })?;
        config.validate(&path)?;
        Ok(config)
    }

    pub fn severity(&self, check: &str, default: CheckSeverity) -> CheckSeverity {
        self.checks.get(check).copied().unwrap_or(default)
    }

    fn validate(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(repo) = &self.repo {
            match repo.visibility {
                Visibility::Public | Visibility::Private | Visibility::Internal => {}
            }
        }
        for (name, description) in &self.map.dirs {
            validate_map_value(path, "map.dirs", name, description)?;
        }
        for (name, description) in &self.map.files {
            validate_map_value(path, "map.files", name, description)?;
            if name.contains('/') || name.contains('\\') {
                return Err(ConfigError::Invalid {
                    path: path.to_owned(),
                    message: format!("map.files key {name:?} must name a root file"),
                });
            }
        }
        for entry in &self.generated.entry {
            if entry.name.trim().is_empty() || entry.command.trim().is_empty() {
                return Err(ConfigError::Invalid {
                    path: path.to_owned(),
                    message: "generated entries require non-empty name and command".to_owned(),
                });
            }
        }
        for ignored in &self.ignore {
            if ignored.check.trim().is_empty()
                || ignored.paths.is_empty()
                || ignored.reason.trim().is_empty()
            {
                return Err(ConfigError::Invalid {
                    path: path.to_owned(),
                    message: "ignore entries require check, paths, and reason".to_owned(),
                });
            }
        }
        for item in &self.deferred.item {
            item.validate(path)?;
        }
        Ok(())
    }
}

impl DeferredItem {
    fn validate(&self, path: &Path) -> Result<(), ConfigError> {
        if self.item.trim().is_empty()
            || self.registered.trim().is_empty()
            || self.last_deferred.trim().is_empty()
            || self.reason.trim().is_empty()
            || self.trigger.trim().is_empty()
            || !matches!(self.effort.as_str(), "S" | "M" | "L")
            || self
                .blocking_gate
                .as_deref()
                .is_some_and(|gate| gate.trim().is_empty())
        {
            return Err(ConfigError::Invalid {
                path: path.to_owned(),
                message: "deferred items require non-empty fields and effort S, M, or L".to_owned(),
            });
        }
        for step in &self.steps {
            if step.step.trim().is_empty() || step.date.trim().is_empty() {
                return Err(ConfigError::Invalid {
                    path: path.to_owned(),
                    message: "deferred steps require non-empty step and date".to_owned(),
                });
            }
        }
        if self.effort == "L" && self.steps.is_empty() {
            return Err(ConfigError::Invalid {
                path: path.to_owned(),
                message: "L deferred items require at least one step".to_owned(),
            });
        }
        Ok(())
    }
}

fn validate_map_value(
    path: &Path,
    section: &str,
    name: &str,
    description: &str,
) -> Result<(), ConfigError> {
    let candidate = Path::new(name);
    if name.trim().is_empty()
        || candidate.is_absolute()
        || candidate
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(ConfigError::Invalid {
            path: path.to_owned(),
            message: format!("{section} key {name:?} must be a relative path without .."),
        });
    }
    if description.trim().is_empty() || description.contains(['\n', '\r']) {
        return Err(ConfigError::Invalid {
            path: path.to_owned(),
            message: format!("{section}.{name} must be a non-empty one-line description"),
        });
    }
    Ok(())
}

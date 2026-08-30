use std::fmt;

use clap::ValueEnum;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Human,
    Json,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Warn,
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Warn => formatter.write_str("warning"),
            Self::Error => formatter.write_str("error"),
        }
    }
}

#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub findings: Vec<Finding>,
}

#[derive(Debug, Serialize)]
pub struct Finding {
    pub check: String,
    pub severity: Severity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl Finding {
    pub fn new(
        check: &str,
        severity: Severity,
        message: impl Into<String>,
        path: Option<String>,
    ) -> Self {
        Self {
            check: check.to_owned(),
            severity,
            message: message.into(),
            path,
        }
    }
}

impl Report {
    pub fn push(&mut self, finding: Finding) {
        self.findings.push(finding);
    }

    pub fn extend(&mut self, findings: impl IntoIterator<Item = Finding>) {
        self.findings.extend(findings);
    }

    pub fn has_error(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| finding.severity == Severity::Error)
    }

    pub fn has_warning(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| finding.severity == Severity::Warn)
    }

    pub fn write(&self, format: OutputFormat) -> Result<(), serde_json::Error> {
        match format {
            OutputFormat::Human => {
                if self.findings.is_empty() {
                    println!("repolint: clean");
                } else {
                    for finding in &self.findings {
                        let location = finding
                            .path
                            .as_deref()
                            .map(|path| format!("{path}: "))
                            .unwrap_or_default();
                        println!(
                            "{} [{}] {}{}",
                            finding.severity, finding.check, location, finding.message
                        );
                    }
                }
            }
            OutputFormat::Json => {
                println!("{}", serde_json::to_string_pretty(self)?);
            }
        }
        Ok(())
    }
}

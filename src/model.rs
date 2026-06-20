//! Core data model: per-registry stats for a package, normalized across the
//! registries' very different download reporting.

use serde::Serialize;

/// A package registry downstat can query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Registry {
    #[serde(rename = "crates.io")]
    Crates,
    Pypi,
    Npm,
    Github,
}

impl Registry {
    pub fn as_str(self) -> &'static str {
        match self {
            Registry::Crates => "crates.io",
            Registry::Pypi => "pypi",
            Registry::Npm => "npm",
            Registry::Github => "github",
        }
    }

    /// Every registry, in display order.
    pub fn all() -> [Registry; 4] {
        [
            Registry::Crates,
            Registry::Pypi,
            Registry::Npm,
            Registry::Github,
        ]
    }

    /// Parse a `--only` value.
    pub fn parse(s: &str) -> Option<Registry> {
        match s.to_ascii_lowercase().as_str() {
            "crates" | "crates.io" | "cargo" => Some(Registry::Crates),
            "pypi" | "pip" => Some(Registry::Pypi),
            "npm" => Some(Registry::Npm),
            "github" | "gh" | "releases" => Some(Registry::Github),
            _ => None,
        }
    }
}

/// Download counts, normalized. Registries report different windows, so the
/// `recent` window is labelled rather than assumed comparable across rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Downloads {
    /// All-time total, where the registry exposes it (crates.io, GitHub releases).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Downloads over the `window` below, where the registry exposes it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent: Option<u64>,
    /// Human label for the `recent` window (e.g. "90d", "30d", "releases").
    pub window: &'static str,
}

/// One registry's view of a package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryStat {
    pub registry: Registry,
    /// Whether the package is published on this registry.
    pub found: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloads: Option<Downloads>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// A human note, e.g. why downloads are unavailable on this registry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl RegistryStat {
    /// A "not published here" stat.
    pub fn absent(registry: Registry) -> Self {
        RegistryStat {
            registry,
            found: false,
            version: None,
            downloads: None,
            url: None,
            note: None,
        }
    }
}

/// Everything known about one package across registries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PackageReport {
    pub name: String,
    pub registries: Vec<RegistryStat>,
}

impl PackageReport {
    /// True when the package was found on at least one registry.
    pub fn found_anywhere(&self) -> bool {
        self.registries.iter().any(|r| r.found)
    }
}

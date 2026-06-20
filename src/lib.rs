//! downstat: downloads + latest version for your packages across crates.io,
//! PyPI, npm and GitHub releases, in one view.
//!
//! The whole pipeline is reachable through [`run`], which is generic over the
//! [`Http`] seam so tests drive it with canned responses (no network).

mod error;
mod http;
mod model;
mod output;
mod registries;
pub mod schema;

pub use error::DownstatError;
pub use http::{Http, ReqwestHttp};
pub use model::{Downloads, PackageReport, Registry, RegistryStat};

use std::thread;

/// Rendered output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Table,
    Json,
}

/// A complete downstat request.
#[derive(Debug, Clone)]
pub struct Request {
    pub names: Vec<String>,
    /// Restrict output to these registries (None = all).
    pub only: Option<Vec<Registry>>,
    pub format: OutputFormat,
}

/// Run a downstat request and return the rendered output (no trailing newline).
pub fn run(http: &dyn Http, req: &Request) -> Result<String, DownstatError> {
    let reports: Vec<PackageReport> = req
        .names
        .iter()
        .map(|name| fetch_package(http, name))
        .collect();

    // A single-name query that turned up nothing is a clear exit-1 "not found".
    if req.names.len() == 1 && !reports[0].found_anywhere() {
        return Err(DownstatError::NoData {
            name: req.names[0].clone(),
        });
    }
    Ok(output::render(&reports, req.only.as_deref(), req.format))
}

/// Fetch one package across registries. crates.io / PyPI / npm run in parallel;
/// GitHub releases follow (its repo is derived from crates.io metadata). A
/// single registry's failure becomes a note rather than failing the report.
fn fetch_package(http: &dyn Http, name: &str) -> PackageReport {
    let (crates_res, pypi_res, npm_res) = thread::scope(|s| {
        let c = s.spawn(|| registries::crates_io(http, name));
        let p = s.spawn(|| registries::pypi(http, name));
        let n = s.spawn(|| registries::npm(http, name));
        (c.join().unwrap(), p.join().unwrap(), n.join().unwrap())
    });

    let (crates_stat, repo) = match crates_res {
        Ok((stat, repo)) => (stat, repo),
        Err(e) => (errored(Registry::Crates, &e), None),
    };
    let pypi_stat = pypi_res.unwrap_or_else(|e| errored(Registry::Pypi, &e));
    let npm_stat = npm_res.unwrap_or_else(|e| errored(Registry::Npm, &e));
    let github_stat = match repo {
        Some((owner, repo)) => registries::github_releases(http, &owner, &repo)
            .unwrap_or_else(|e| errored(Registry::Github, &e)),
        None => RegistryStat::absent(Registry::Github),
    };

    PackageReport {
        name: name.to_string(),
        registries: vec![crates_stat, pypi_stat, npm_stat, github_stat],
    }
}

fn errored(registry: Registry, e: &DownstatError) -> RegistryStat {
    RegistryStat {
        registry,
        found: false,
        version: None,
        downloads: None,
        url: None,
        note: Some(e.to_string()),
    }
}

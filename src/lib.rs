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

struct FetchResult {
    report: PackageReport,
    errors: Vec<(Vec<Registry>, DownstatError)>,
}

/// Run a downstat request and return the rendered output (no trailing newline).
pub fn run(http: &dyn Http, req: &Request) -> Result<String, DownstatError> {
    let mut fetched: Vec<FetchResult> = req
        .names
        .iter()
        .map(|name| fetch_package(http, name))
        .collect();

    if req.names.len() == 1 {
        let selected = |registry: Registry| {
            req.only
                .as_ref()
                .is_none_or(|only| only.contains(&registry))
        };
        let found = fetched[0]
            .report
            .registries
            .iter()
            .any(|stat| selected(stat.registry) && stat.found);
        if !found {
            if let Some(index) = fetched[0]
                .errors
                .iter()
                .position(|(affected, _)| affected.iter().copied().any(&selected))
            {
                return Err(fetched[0].errors.swap_remove(index).1);
            }
            return Err(DownstatError::NoData {
                name: req.names[0].clone(),
            });
        }
    }

    let reports: Vec<_> = fetched.into_iter().map(|result| result.report).collect();
    Ok(output::render(&reports, req.only.as_deref(), req.format))
}

/// Fetch one package across registries. crates.io / PyPI / npm run in parallel;
/// GitHub releases follow (its repo is derived from crates.io metadata). A
/// single registry's failure becomes a note rather than failing the report.
fn fetch_package(http: &dyn Http, name: &str) -> FetchResult {
    let (crates_res, pypi_res, npm_res) = thread::scope(|s| {
        let c = s.spawn(|| registries::crates_io(http, name));
        let p = s.spawn(|| registries::pypi(http, name));
        let n = s.spawn(|| registries::npm(http, name));
        (c.join().unwrap(), p.join().unwrap(), n.join().unwrap())
    });

    let mut errors = Vec::new();
    let (crates_stat, repo) = match crates_res {
        Ok((stat, repo)) => (stat, repo),
        Err(e) => {
            let stat = errored(Registry::Crates, &e);
            // GitHub lookup depends on repository metadata from crates.io.
            errors.push((vec![Registry::Crates, Registry::Github], e));
            (stat, None)
        }
    };
    let pypi_stat = match pypi_res {
        Ok(stat) => stat,
        Err(e) => {
            let stat = errored(Registry::Pypi, &e);
            errors.push((vec![Registry::Pypi], e));
            stat
        }
    };
    let npm_stat = match npm_res {
        Ok(stat) => stat,
        Err(e) => {
            let stat = errored(Registry::Npm, &e);
            errors.push((vec![Registry::Npm], e));
            stat
        }
    };
    let github_stat = match repo {
        Some((owner, repo)) => match registries::github_releases(http, &owner, &repo) {
            Ok(stat) => stat,
            Err(e) => {
                let stat = errored(Registry::Github, &e);
                errors.push((vec![Registry::Github], e));
                stat
            }
        },
        None => RegistryStat::absent(Registry::Github),
    };

    FetchResult {
        report: PackageReport {
            name: name.to_string(),
            registries: vec![crates_stat, pypi_stat, npm_stat, github_stat],
        },
        errors,
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

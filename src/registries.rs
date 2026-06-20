//! Per-registry fetch + parse. Each function takes an [`Http`] and a package
//! name and returns that registry's [`RegistryStat`]. Downloads are normalized
//! into [`Downloads`]; where a registry exposes no count, a `note` explains why.

use crate::error::DownstatError;
use crate::http::Http;
use crate::model::{Downloads, Registry, RegistryStat};
use serde_json::Value;

fn parse(registry: &'static str, body: &str) -> Result<Value, DownstatError> {
    serde_json::from_str(body).map_err(|e| DownstatError::Parse {
        registry,
        message: e.to_string(),
    })
}

/// crates.io: total + 90-day downloads, latest version, and the source repo
/// (used to find GitHub releases).
pub fn crates_io(
    http: &dyn Http,
    name: &str,
) -> Result<(RegistryStat, Option<(String, String)>), DownstatError> {
    let url = format!("https://crates.io/api/v1/crates/{name}");
    let Some(body) = http.get(&url)? else {
        return Ok((RegistryStat::absent(Registry::Crates), None));
    };
    let v = parse("crates.io", &body)?;
    let c = &v["crate"];
    let repo = c["repository"].as_str().and_then(github_repo);
    let stat = RegistryStat {
        registry: Registry::Crates,
        found: true,
        version: c["max_version"].as_str().map(str::to_string),
        downloads: Some(Downloads {
            total: c["downloads"].as_u64(),
            recent: c["recent_downloads"].as_u64(),
            window: "90d",
        }),
        url: Some(format!("https://crates.io/crates/{name}")),
        note: None,
    };
    Ok((stat, repo))
}

/// PyPI: latest version from the JSON API, downloads from pypistats.org (the
/// PyPI API itself no longer exposes counts).
pub fn pypi(http: &dyn Http, name: &str) -> Result<RegistryStat, DownstatError> {
    let Some(body) = http.get(&format!("https://pypi.org/pypi/{name}/json"))? else {
        return Ok(RegistryStat::absent(Registry::Pypi));
    };
    let v = parse("pypi", &body)?;
    let version = v["info"]["version"].as_str().map(str::to_string);

    let (recent, note) =
        match http.get(&format!("https://pypistats.org/api/packages/{name}/recent")) {
            Ok(Some(b)) => (
                parse("pypistats", &b)
                    .ok()
                    .and_then(|d| d["data"]["last_month"].as_u64()),
                None,
            ),
            Ok(None) | Err(_) => (
                None,
                Some("downloads via pypistats unavailable".to_string()),
            ),
        };
    Ok(RegistryStat {
        registry: Registry::Pypi,
        found: true,
        version,
        downloads: Some(Downloads {
            total: None,
            recent,
            window: "30d",
        }),
        url: Some(format!("https://pypi.org/project/{name}/")),
        note,
    })
}

/// npm: latest version from the registry, last-month downloads from the
/// downloads API.
pub fn npm(http: &dyn Http, name: &str) -> Result<RegistryStat, DownstatError> {
    let Some(body) = http.get(&format!("https://registry.npmjs.org/{name}"))? else {
        return Ok(RegistryStat::absent(Registry::Npm));
    };
    let v = parse("npm", &body)?;
    let version = v["dist-tags"]["latest"].as_str().map(str::to_string);

    let recent = match http.get(&format!(
        "https://api.npmjs.org/downloads/point/last-month/{name}"
    ))? {
        Some(b) => parse("npm", &b).ok().and_then(|d| d["downloads"].as_u64()),
        None => None,
    };
    Ok(RegistryStat {
        registry: Registry::Npm,
        found: true,
        version,
        downloads: Some(Downloads {
            total: None,
            recent,
            window: "30d",
        }),
        url: Some(format!("https://www.npmjs.com/package/{name}")),
        note: None,
    })
}

/// GitHub releases: latest tag and the summed asset download counts.
pub fn github_releases(
    http: &dyn Http,
    owner: &str,
    repo: &str,
) -> Result<RegistryStat, DownstatError> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/latest");
    let Some(body) = http.get(&url)? else {
        return Ok(RegistryStat::absent(Registry::Github));
    };
    let v = parse("github", &body)?;
    let total: u64 = v["assets"]
        .as_array()
        .map(|a| a.iter().filter_map(|x| x["download_count"].as_u64()).sum())
        .unwrap_or(0);
    Ok(RegistryStat {
        registry: Registry::Github,
        found: true,
        version: v["tag_name"].as_str().map(str::to_string),
        downloads: Some(Downloads {
            total: Some(total),
            recent: None,
            window: "releases",
        }),
        url: Some(format!("https://github.com/{owner}/{repo}/releases")),
        note: None,
    })
}

/// Extract `(owner, repo)` from a GitHub URL, if it is one.
fn github_repo(url: &str) -> Option<(String, String)> {
    let rest = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
        .or_else(|| url.strip_prefix("git+https://github.com/"))?;
    let mut parts = rest.trim_end_matches('/').splitn(3, '/');
    let owner = parts.next()?.to_string();
    let repo = parts.next()?.trim_end_matches(".git").to_string();
    if owner.is_empty() || repo.is_empty() {
        None
    } else {
        Some((owner, repo))
    }
}

#[cfg(test)]
mod tests {
    use super::github_repo;

    #[test]
    fn parses_github_repo_from_various_urls() {
        assert_eq!(
            github_repo("https://github.com/rvben/onym"),
            Some(("rvben".into(), "onym".into()))
        );
        assert_eq!(
            github_repo("https://github.com/rvben/onym.git"),
            Some(("rvben".into(), "onym".into()))
        );
        assert_eq!(
            github_repo("https://github.com/rvben/onym/tree/main"),
            Some(("rvben".into(), "onym".into()))
        );
        assert_eq!(github_repo("https://gitlab.com/x/y"), None);
    }
}

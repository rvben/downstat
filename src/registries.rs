//! Per-registry fetch + parse. Each function takes an [`Http`] and a package
//! name and returns that registry's [`RegistryStat`]. Downloads are normalized
//! into [`Downloads`]; where a registry exposes no count, a `note` explains why.

use crate::error::DownstatError;
use crate::http::Http;
use crate::model::{Downloads, Registry, RegistryStat};
use serde_json::Value;
use std::collections::HashSet;

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

/// Page size for the GitHub releases list (the API maximum).
const GITHUB_RELEASES_PER_PAGE: usize = 100;

/// Most releases counted for one repo. A server that keeps returning full pages
/// past this is a malformed response, not a repo with unbounded releases.
const GITHUB_RELEASES_MAX: usize = 10_000;

/// GitHub releases: the latest tag, and the all-time download total summed over
/// the assets of every published release. A repo whose only published releases
/// are prereleases has no latest release but is still found, without a version.
pub fn github_releases(
    http: &dyn Http,
    owner: &str,
    repo: &str,
) -> Result<RegistryStat, DownstatError> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/latest");
    let latest = match http.get(&url)? {
        Some(body) => Some(parse("github", &body)?),
        None => None,
    };
    let Some(releases) = github_release_downloads(http, owner, repo, latest.is_some())? else {
        return Ok(RegistryStat::absent(Registry::Github));
    };
    if latest.is_none() && releases.published == 0 {
        return Ok(RegistryStat::absent(Registry::Github));
    }
    let version = latest
        .as_ref()
        .and_then(|l| l["tag_name"].as_str())
        .map(str::to_string);
    let note = latest
        .is_none()
        .then(|| "only prereleases are published".to_string());
    Ok(RegistryStat {
        registry: Registry::Github,
        found: true,
        version,
        downloads: Some(Downloads {
            total: Some(releases.total),
            recent: None,
            window: "releases",
        }),
        url: Some(format!("https://github.com/{owner}/{repo}/releases")),
        note,
    })
}

/// The published (non-draft) releases of a repo: how many, and their summed
/// asset downloads.
struct ReleaseDownloads {
    published: usize,
    total: u64,
}

/// Sum `download_count` over every asset of every published release, following
/// pages until one comes back short. Drafts (listed only to callers with push
/// access) are skipped, and a release seen on two pages (the list shifted while
/// it was read) is counted once. A 404 on the first page means the repo is
/// absent (`None`) unless `latest_found` says it exists. A malformed page, a 404
/// on a later page, an overflowing sum or a history past [`GITHUB_RELEASES_MAX`]
/// is a parse error, never a partial or wrong total.
fn github_release_downloads(
    http: &dyn Http,
    owner: &str,
    repo: &str,
    latest_found: bool,
) -> Result<Option<ReleaseDownloads>, DownstatError> {
    let malformed = |message: String| DownstatError::Parse {
        registry: "github",
        message,
    };
    let mut total = 0u64;
    let mut published = 0usize;
    let mut seen = HashSet::new();
    // The page after the cap must come back empty for the history to fit.
    let past_cap = GITHUB_RELEASES_MAX / GITHUB_RELEASES_PER_PAGE + 1;
    for page in 1..=past_cap {
        let url = format!(
            "https://api.github.com/repos/{owner}/{repo}/releases?per_page={GITHUB_RELEASES_PER_PAGE}&page={page}"
        );
        let Some(body) = http.get(&url)? else {
            if page == 1 && !latest_found {
                return Ok(None);
            }
            return Err(malformed(format!(
                "{url} returned 404 after the repo's releases were found"
            )));
        };
        let releases = parse("github", &body)?;
        let releases = releases
            .as_array()
            .ok_or_else(|| malformed(format!("{url} did not return a list of releases")))?;
        if releases.len() > GITHUB_RELEASES_PER_PAGE {
            return Err(malformed(format!(
                "{url} returned {} releases, more than the {GITHUB_RELEASES_PER_PAGE} requested",
                releases.len()
            )));
        }
        if page == past_cap && !releases.is_empty() {
            break;
        }
        for release in releases {
            let id = release["id"]
                .as_u64()
                .ok_or_else(|| malformed(format!("a release in {url} has no id")))?;
            let draft = release["draft"]
                .as_bool()
                .ok_or_else(|| malformed(format!("release {id} in {url} has no draft flag")))?;
            if draft || !seen.insert(id) {
                continue;
            }
            published += 1;
            let assets = release["assets"]
                .as_array()
                .ok_or_else(|| malformed(format!("release {id} in {url} has no assets list")))?;
            for asset in assets {
                let count = asset["download_count"].as_u64().ok_or_else(|| {
                    malformed(format!("an asset of release {id} has no download_count"))
                })?;
                total = total
                    .checked_add(count)
                    .ok_or_else(|| malformed(format!("{owner}/{repo} download total overflows")))?;
            }
        }
        if releases.len() < GITHUB_RELEASES_PER_PAGE {
            return Ok(Some(ReleaseDownloads { published, total }));
        }
    }
    Err(malformed(format!(
        "{owner}/{repo} has more than {GITHUB_RELEASES_MAX} releases"
    )))
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

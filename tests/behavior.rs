//! Behavior tests: drive `run()` with a fake HTTP layer returning canned
//! registry responses, so all per-registry parsing and aggregation is tested
//! offline.

use std::collections::HashMap;

use downstat::{DownstatError, Http, OutputFormat, Registry, Request, run};

/// Maps URL -> Some(body) for 200, or absent/None for 404.
struct FakeHttp(HashMap<String, String>);

impl FakeHttp {
    fn new(pairs: &[(&str, &str)]) -> Self {
        FakeHttp(
            pairs
                .iter()
                .map(|(u, b)| (u.to_string(), b.to_string()))
                .collect(),
        )
    }
}

impl Http for FakeHttp {
    fn get(&self, url: &str) -> Result<Option<String>, DownstatError> {
        Ok(self.0.get(url).cloned())
    }
}

struct UnavailableHttp;

impl Http for UnavailableHttp {
    fn get(&self, url: &str) -> Result<Option<String>, DownstatError> {
        Err(DownstatError::Http {
            message: format!("{url} is unavailable"),
        })
    }
}

fn req(name: &str, only: Option<Vec<Registry>>) -> Request {
    Request {
        names: vec![name.to_string()],
        only,
        format: OutputFormat::Json,
    }
}

fn json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).expect("output is JSON")
}

/// Canned responses for a package present on every registry.
fn full_fixture(name: &str) -> FakeHttp {
    FakeHttp::new(&[
        (
            &format!("https://crates.io/api/v1/crates/{name}"),
            r#"{"crate":{"max_version":"0.1.2","downloads":1234,"recent_downloads":56,"repository":"https://github.com/rvben/onym"}}"#,
        ),
        (
            &format!("https://pypi.org/pypi/{name}/json"),
            r#"{"info":{"version":"0.1.2"}}"#,
        ),
        (
            &format!("https://pypistats.org/api/packages/{name}/recent"),
            r#"{"data":{"last_day":3,"last_week":20,"last_month":42}}"#,
        ),
        (
            &format!("https://registry.npmjs.org/{name}"),
            r#"{"dist-tags":{"latest":"9.9.9"}}"#,
        ),
        (
            &format!("https://api.npmjs.org/downloads/point/last-month/{name}"),
            r#"{"downloads":10,"package":"x"}"#,
        ),
        (
            "https://api.github.com/repos/rvben/onym/releases/latest",
            r#"{"tag_name":"v0.1.2","assets":[{"download_count":7},{"download_count":3}]}"#,
        ),
        (
            "https://api.github.com/repos/rvben/onym/releases?per_page=100&page=1",
            r#"[{"id":3,"draft":false,"tag_name":"v0.1.2","assets":[{"download_count":7},{"download_count":3}]},
                {"id":2,"draft":false,"tag_name":"v0.1.1","assets":[{"download_count":90}]},
                {"id":1,"draft":false,"tag_name":"v0.1.0","assets":[]}]"#,
        ),
    ])
}

fn reg<'a>(v: &'a serde_json::Value, registry: &str) -> &'a serde_json::Value {
    v["packages"][0]["registries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["registry"] == registry)
        .unwrap_or_else(|| panic!("registry {registry} missing"))
}

#[test]
fn aggregates_downloads_and_versions_across_registries() {
    let v = json(&run(&full_fixture("onym"), &req("onym", None)).unwrap());
    assert_eq!(v["packages"][0]["name"], "onym");

    let c = reg(&v, "crates.io");
    assert_eq!(c["version"], "0.1.2");
    assert_eq!(c["downloads"]["total"], 1234);
    assert_eq!(c["downloads"]["recent"], 56);

    assert_eq!(reg(&v, "pypi")["downloads"]["recent"], 42);
    assert_eq!(reg(&v, "npm")["version"], "9.9.9");
    assert_eq!(reg(&v, "npm")["downloads"]["recent"], 10);

    // GitHub repo derived from crates.io metadata; asset counts summed over
    // every release, not only the latest.
    let g = reg(&v, "github");
    assert_eq!(g["version"], "v0.1.2");
    assert_eq!(g["downloads"]["total"], 100);
}

/// crates.io metadata pointing at `rvben/<name>` on GitHub, a latest release,
/// and the given release-list pages; every other registry 404s.
fn paged(name: &str, pages: Vec<String>) -> FakeHttp {
    let mut pairs = vec![
        (
            format!("https://crates.io/api/v1/crates/{name}"),
            format!(
                r#"{{"crate":{{"max_version":"1.0.0","downloads":1,"recent_downloads":1,"repository":"https://github.com/rvben/{name}"}}}}"#
            ),
        ),
        (
            format!("https://api.github.com/repos/rvben/{name}/releases/latest"),
            r#"{"tag_name":"v-latest","assets":[]}"#.to_string(),
        ),
    ];
    pairs.extend(pages.into_iter().enumerate().map(|(i, body)| {
        (
            format!(
                "https://api.github.com/repos/rvben/{name}/releases?per_page=100&page={}",
                i + 1
            ),
            body,
        )
    }));
    FakeHttp(pairs.into_iter().collect())
}

/// One published release with the given asset download counts.
fn release(id: u64, counts: &[u64]) -> String {
    let assets: Vec<String> = counts
        .iter()
        .map(|c| format!(r#"{{"download_count":{c}}}"#))
        .collect();
    format!(
        r#"{{"id":{id},"draft":false,"assets":[{}]}}"#,
        assets.join(",")
    )
}

fn page(releases: &[String]) -> String {
    format!("[{}]", releases.join(","))
}

/// `n` releases with ids from `first_id`, one asset of `count` downloads each.
fn run_of(first_id: u64, n: u64, count: u64) -> Vec<String> {
    (first_id..first_id + n)
        .map(|id| release(id, &[count]))
        .collect()
}

fn github_only() -> Option<Vec<Registry>> {
    Some(vec![Registry::Github])
}

fn github_total(http: &FakeHttp, name: &str) -> serde_json::Value {
    let v = json(&run(http, &req(name, github_only())).unwrap());
    reg(&v, "github")["downloads"]["total"].clone()
}

fn github_error(http: &FakeHttp, name: &str) -> DownstatError {
    run(http, &req(name, github_only())).unwrap_err()
}

#[test]
fn github_total_counts_every_release_not_only_the_latest() {
    // A release published hours ago has almost no downloads of its own; the
    // all-time total must still include the releases before it.
    let http = paged(
        "rg",
        vec![page(&[release(2, &[5]), release(1, &[2_000_000, 1])])],
    );
    assert_eq!(github_total(&http, "rg"), 2_000_006);
}

#[test]
fn github_total_follows_pages_until_a_short_page() {
    let http = paged(
        "many",
        vec![page(&run_of(1, 100, 1)), page(&[release(101, &[40])])],
    );
    assert_eq!(github_total(&http, "many"), 140);
}

#[test]
fn github_total_accepts_an_empty_page_after_a_full_one() {
    let http = paged("exact", vec![page(&run_of(1, 100, 2)), "[]".into()]);
    assert_eq!(github_total(&http, "exact"), 200);
}

#[test]
fn github_total_reads_a_history_of_exactly_the_release_cap() {
    let mut pages: Vec<String> = (0..100)
        .map(|p| page(&run_of(p * 100 + 1, 100, 1)))
        .collect();
    pages.push("[]".into());
    assert_eq!(github_total(&paged("cap", pages), "cap"), 10_000);
}

#[test]
fn github_history_past_the_release_cap_is_an_error_not_a_partial_total() {
    // 10,001 releases: the page after the cap holds one release.
    let mut pages: Vec<String> = (0..100)
        .map(|p| page(&run_of(p * 100 + 1, 100, 1)))
        .collect();
    pages.push(page(&[release(10_001, &[1])]));
    let err = github_error(&paged("huge", pages), "huge");
    assert!(matches!(err, DownstatError::Parse { .. }), "{err:?}");
}

#[test]
fn github_release_repeated_across_pages_is_counted_once() {
    // A release published between page requests shifts release 100 onto page 2.
    let http = paged(
        "shift",
        vec![
            page(&run_of(1, 100, 1)),
            page(&[release(100, &[1]), release(101, &[7])]),
        ],
    );
    assert_eq!(github_total(&http, "shift"), 107);
}

#[test]
fn github_total_includes_prereleases() {
    let pre =
        r#"{"id":2,"draft":false,"prerelease":true,"assets":[{"download_count":30}]}"#.to_string();
    let http = paged("pre", vec![page(&[pre, release(1, &[4])])]);
    assert_eq!(github_total(&http, "pre"), 34);
}

#[test]
fn github_page_larger_than_requested_is_an_error() {
    // 101 releases on one page would slip past a cap that only counts pages.
    let http = paged("oversized", vec![page(&run_of(1, 101, 1)), "[]".into()]);
    let err = github_error(&http, "oversized");
    assert!(matches!(err, DownstatError::Parse { .. }), "{err:?}");
}

#[test]
fn github_later_page_404_is_an_error_not_a_partial_total() {
    // Page 1 is full, page 2 is absent from the fake (a 404).
    let http = paged("gone", vec![page(&run_of(1, 100, 1))]);
    let err = github_error(&http, "gone");
    assert!(matches!(err, DownstatError::Parse { .. }), "{err:?}");
}

#[test]
fn github_total_skips_draft_releases() {
    let draft = r#"{"id":9,"draft":true,"assets":[{"download_count":1000}]}"#.to_string();
    let http = paged("drafts", vec![page(&[draft, release(1, &[4])])]);
    assert_eq!(github_total(&http, "drafts"), 4);
}

#[test]
fn overflowing_github_total_is_an_error_not_a_capped_value() {
    let http = paged("big", vec![page(&[release(1, &[u64::MAX, 1])])]);
    let err = github_error(&http, "big");
    assert!(matches!(err, DownstatError::Parse { .. }), "{err:?}");
}

#[test]
fn malformed_github_release_list_is_a_parse_error_not_a_partial_total() {
    for (label, pages) in [
        (
            "not a list",
            vec![r#"{"message":"rate limited"}"#.to_string()],
        ),
        (
            "release without an id",
            vec![r#"[{"draft":false,"assets":[]}]"#.into()],
        ),
        (
            "release without a draft flag",
            vec![r#"[{"id":1,"assets":[]}]"#.into()],
        ),
        (
            "non-boolean draft flag",
            vec![r#"[{"id":1,"draft":"true","assets":[]}]"#.into()],
        ),
        (
            "release without assets",
            vec![r#"[{"id":1,"draft":false}]"#.into()],
        ),
        (
            "asset without a count",
            vec![r#"[{"id":1,"draft":false,"assets":[{"name":"x"}]}]"#.into()],
        ),
        (
            "malformed later page",
            vec![page(&run_of(1, 100, 5)), r#"{"message":"oops"}"#.into()],
        ),
    ] {
        let err = github_error(&paged("bad", pages), "bad");
        assert!(
            matches!(err, DownstatError::Parse { .. }),
            "{label}: expected a parse error, got {err:?}"
        );
    }
}

#[test]
fn absent_registries_are_marked_not_found() {
    // Only on crates.io (repo present so GitHub is checked, but 404 there).
    let http = FakeHttp::new(&[(
        "https://crates.io/api/v1/crates/solo",
        r#"{"crate":{"max_version":"1.0.0","downloads":5,"recent_downloads":1,"repository":"https://github.com/rvben/solo"}}"#,
    )]);
    let v = json(&run(&http, &req("solo", None)).unwrap());
    assert_eq!(reg(&v, "crates.io")["found"], true);
    assert_eq!(reg(&v, "pypi")["found"], false);
    assert_eq!(reg(&v, "npm")["found"], false);
    assert_eq!(reg(&v, "github")["found"], false);
}

#[test]
fn not_found_anywhere_is_no_data_exit_1() {
    let http = FakeHttp::new(&[]); // everything 404s
    let err = run(&http, &req("ghost", None)).unwrap_err();
    assert!(matches!(err, DownstatError::NoData { .. }));
    assert_eq!(err.exit_code(), 1);
}

#[test]
fn unavailable_selected_registry_is_not_reported_as_no_data() {
    let err = run(
        &UnavailableHttp,
        &req("serde", Some(vec![Registry::Crates])),
    )
    .unwrap_err();
    assert!(matches!(err, DownstatError::Http { .. }));
    assert_eq!(err.kind(), "unavailable");
    assert_eq!(err.exit_code(), 2);
}

#[test]
fn malformed_selected_registry_response_preserves_parse_error() {
    let http = FakeHttp::new(&[("https://crates.io/api/v1/crates/broken", "not json")]);
    let err = run(&http, &req("broken", Some(vec![Registry::Crates]))).unwrap_err();
    assert!(matches!(err, DownstatError::Parse { .. }));
    assert_eq!(err.kind(), "parse");
    assert_eq!(err.exit_code(), 2);
}

#[test]
fn only_filter_restricts_output() {
    let v = json(
        &run(
            &full_fixture("onym"),
            &req("onym", Some(vec![Registry::Crates])),
        )
        .unwrap(),
    );
    let regs = v["packages"][0]["registries"].as_array().unwrap();
    assert_eq!(regs.len(), 1);
    assert_eq!(regs[0]["registry"], "crates.io");
}

#[test]
fn pypi_without_pypistats_notes_the_gap() {
    // PyPI metadata present, pypistats 404 -> version shown, downloads noted.
    let http = FakeHttp::new(&[(
        "https://pypi.org/pypi/lonely/json",
        r#"{"info":{"version":"2.0.0"}}"#,
    )]);
    let v = json(&run(&http, &req("lonely", None)).unwrap());
    let p = reg(&v, "pypi");
    assert_eq!(p["version"], "2.0.0");
    assert!(p["note"].as_str().unwrap().contains("pypistats"));
}

#[test]
fn table_output_is_not_json() {
    let r = Request {
        format: OutputFormat::Table,
        ..req("onym", None)
    };
    let out = run(&full_fixture("onym"), &r).unwrap();
    assert!(out.contains("PACKAGE") && out.contains("crates.io") && out.contains("1,234"));
    assert!(serde_json::from_str::<serde_json::Value>(&out).is_err());
}

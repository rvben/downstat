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

    // GitHub repo derived from crates.io metadata; asset counts summed.
    let g = reg(&v, "github");
    assert_eq!(g["version"], "v0.1.2");
    assert_eq!(g["downloads"]["total"], 10);
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

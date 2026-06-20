//! Tolerant real-network tests: confirm each registry's *real* response shape
//! still parses, against packages large enough that their lower bounds are
//! stable. They skip (rather than fail) when the network is unavailable, so a
//! flaky/offline runner does not break the build - while still catching real
//! API-shape drift when the network is up (CI runners have network).

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_downstat");

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run(args: &[&str]) -> Out {
    let o = Command::new(BIN)
        .args(args)
        .output()
        .expect("spawn downstat");
    Out {
        code: o.status.code().unwrap(),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

/// Returns the single registry stat, or None to skip (network down).
fn single_registry(args: &[&str], registry: &str) -> Option<serde_json::Value> {
    let out = run(args);
    if out.code == 2 {
        eprintln!("skipping (network unavailable): {}", out.stderr.trim());
        return None;
    }
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let v: serde_json::Value = serde_json::from_str(&out.stdout).expect("json");
    let r = v["packages"][0]["registries"][0].clone();
    assert_eq!(r["registry"], registry, "unexpected registry in {args:?}");
    assert_eq!(r["found"], true);
    Some(r)
}

#[test]
fn real_pypi_downloads_parse() {
    let Some(r) = single_registry(&["requests", "--only", "pypi", "-o", "json"], "pypi") else {
        return;
    };
    assert!(r["downloads"]["recent"].as_u64().unwrap() > 1_000_000);
}

#[test]
fn real_npm_downloads_parse() {
    let Some(r) = single_registry(&["express", "--only", "npm", "-o", "json"], "npm") else {
        return;
    };
    assert!(r["downloads"]["recent"].as_u64().unwrap() > 1_000_000);
}

#[test]
fn real_github_release_downloads_parse() {
    // GitHub repo is derived from ripgrep's crates.io metadata.
    let Some(r) = single_registry(&["ripgrep", "--only", "github", "-o", "json"], "github") else {
        return;
    };
    assert!(r["downloads"]["total"].as_u64().unwrap() > 1_000_000);
}

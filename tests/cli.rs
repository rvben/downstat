//! End-to-end tests of the compiled binary: the clispec contract, error
//! envelope, exit codes, and one tolerant real-network check.

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_downstat");

struct Output {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run(args: &[&str]) -> Output {
    let out = Command::new(BIN)
        .args(args)
        .output()
        .expect("spawn downstat");
    Output {
        code: out.status.code().unwrap(),
        stdout: String::from_utf8(out.stdout).unwrap(),
        stderr: String::from_utf8(out.stderr).unwrap(),
    }
}

fn error_envelope(stderr: &str) -> serde_json::Value {
    let last = stderr.lines().last().expect("stderr has an error line");
    serde_json::from_str::<serde_json::Value>(last).expect("error envelope is JSON")["error"]
        .clone()
}

#[test]
fn schema_subcommand_is_clispec_v0_2() {
    let out = run(&["schema"]);
    assert_eq!(out.code, 0);
    let v: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(v["clispec"], "0.2");
    assert_eq!(v["name"], "downstat");
}

#[test]
fn help_mentions_schema() {
    let out = run(&["--help"]);
    assert_eq!(out.code, 0);
    assert!(out.stdout.contains("schema"));
}

#[test]
fn no_args_exits_3_with_usage_envelope() {
    let out = run(&[]);
    assert_eq!(out.code, 3);
    assert_eq!(error_envelope(&out.stderr)["kind"], "usage");
}

#[test]
fn bad_flag_exits_3() {
    let out = run(&["serde", "--no-such-flag"]);
    assert_eq!(out.code, 3);
    assert_eq!(error_envelope(&out.stderr)["kind"], "usage");
}

/// Tolerant real-network test: queries a definitely-published crate. If the
/// network is unavailable (exit 2), it skips rather than failing spuriously.
#[test]
fn real_lookup_of_a_known_crate() {
    let out = run(&["serde", "--only", "crates", "-o", "json"]);
    if out.code == 2 {
        eprintln!("skipping: network unavailable ({})", out.stderr.trim());
        return;
    }
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let v: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    let crates = v["packages"][0]["registries"][0].clone();
    assert_eq!(crates["registry"], "crates.io");
    assert_eq!(crates["found"], true);
    assert!(crates["downloads"]["total"].as_u64().unwrap() > 1_000_000);
}

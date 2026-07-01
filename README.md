# downstat

[![CI](https://github.com/rvben/downstat/actions/workflows/ci.yml/badge.svg)](https://github.com/rvben/downstat/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/downstat.svg)](https://crates.io/crates/downstat)
[![clispec compliant](https://img.shields.io/badge/clispec-compliant-3b82f6)](https://clispec.dev)

Downloads and latest version for your published packages across crates.io,
PyPI, npm and GitHub releases - in one view, instead of curling five APIs by
hand.

## Why

If you publish to several registries, "is this tool getting used, and what's
live where?" means visiting crates.io, PyPI, npm, and a GitHub releases page
one at a time. `downstat <name>` answers it in one shot; `downstat --all` does
your whole portfolio. JSON when piped, a table in a terminal.

## Install

```sh
cargo install downstat
```

## Quickstart

```sh
# One package across every registry it's on
downstat dotpick
# PACKAGE   REGISTRY   VERSION   DOWNLOADS
# dotpick   crates.io  0.1.0     1,234 total · 56 (90d)
# dotpick   pypi       0.1.0     42 (30d)
# dotpick   github     v0.1.0    7 (releases)

# JSON for scripting
downstat dotpick | jq '.packages[0].registries[] | {registry, downloads}'

# Just one registry
downstat dotpick --only crates

# Your whole portfolio (from ./downstat.toml)
downstat --all
```

`downstat.toml`:

```toml
packages = ["dotpick", "whatport", "onym", "vership"]
```

## What it reads

| registry | downloads | latest version |
| --- | --- | --- |
| crates.io | total + 90-day | ✓ |
| PyPI | 30-day (via pypistats.org) | ✓ |
| npm | 30-day | ✓ |
| GitHub releases | summed release-asset downloads | ✓ (latest tag) |

The GitHub repo is discovered from crates.io's `repository` field. A
`GITHUB_TOKEN`/`GH_TOKEN` env var (optional) lifts GitHub's unauthenticated
rate limit for large `--all` runs.

Counts are **normalized but not directly comparable** across registries (the
windows differ - that's why each `recent` value is labelled). ghcr and Homebrew
taps expose no public download counts, so they are omitted rather than faked.

## Exit codes

| code | meaning |
| --- | --- |
| `0` | success |
| `1` | the queried name was found on no registry |
| `2` | a network or parse failure |
| `3` | usage error (bad arguments / config) |

## For agents (clispec)

downstat follows [The CLI Spec](https://clispec.dev): structured output on
stdout, structured error envelopes on the last line of stderr, and a `schema`
subcommand whose output validates against `clispec.dev/schema/v0.2.json`
(checked by the test suite). Every command is read-only (`mutating: false`).

## License

MIT

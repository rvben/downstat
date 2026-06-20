//! Rendering package reports as a table (TTY) or JSON (piped).

use crate::OutputFormat;
use crate::model::{Downloads, PackageReport, Registry, RegistryStat};
use serde_json::json;

/// Render reports, optionally filtered to a set of registries.
pub fn render(
    reports: &[PackageReport],
    only: Option<&[Registry]>,
    format: OutputFormat,
) -> String {
    let keep = |r: &&RegistryStat| only.is_none_or(|set| set.contains(&r.registry));
    match format {
        OutputFormat::Json => {
            let packages: Vec<_> = reports
                .iter()
                .map(|p| {
                    let regs: Vec<&RegistryStat> = p.registries.iter().filter(keep).collect();
                    json!({ "name": p.name, "registries": regs })
                })
                .collect();
            json!({ "packages": packages }).to_string()
        }
        OutputFormat::Table => table(reports, only),
    }
}

fn table(reports: &[PackageReport], only: Option<&[Registry]>) -> String {
    let header = format!(
        "{:<18} {:<10} {:<12} {}",
        "PACKAGE", "REGISTRY", "VERSION", "DOWNLOADS"
    );
    let mut rows = vec![header];
    let mut any = false;
    for report in reports {
        for stat in &report.registries {
            if let Some(set) = only
                && !set.contains(&stat.registry)
            {
                continue;
            }
            // Show registries the package is on, plus any that errored.
            if !stat.found && stat.note.is_none() {
                continue;
            }
            any = true;
            rows.push(format!(
                "{:<18} {:<10} {:<12} {}",
                truncate(&report.name, 18),
                stat.registry.as_str(),
                stat.version.as_deref().unwrap_or("-"),
                downloads_cell(stat),
            ));
        }
    }
    if !any {
        rows.push("(no packages found on any registry)".to_string());
    }
    rows.join("\n")
}

fn downloads_cell(stat: &RegistryStat) -> String {
    if let Some(note) = &stat.note {
        return note.clone();
    }
    match &stat.downloads {
        Some(d) => format_downloads(d),
        None => "-".to_string(),
    }
}

fn format_downloads(d: &Downloads) -> String {
    match (d.total, d.recent) {
        (Some(t), Some(r)) => format!("{} total · {} ({})", commas(t), commas(r), d.window),
        (Some(t), None) => format!("{} ({})", commas(t), d.window),
        (None, Some(r)) => format!("{} ({})", commas(r), d.window),
        (None, None) => "-".to_string(),
    }
}

/// Group digits with thousands separators: 1234567 -> "1,234,567".
fn commas(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let keep: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{keep}…")
    }
}

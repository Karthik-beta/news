//! axe-core for C13, run through the site's own Playwright and axe tooling: this spawns
//! `bun site/scripts/axe.ts <bundle dir>`, which opens the bundle's `index.html` in Chromium with the
//! WCAG 2.2 AA rules (the same tags as `site/tests/e2e/fixtures.ts`) and prints
//! `{"violations":[{"id","impact","help","nodes"}]}`.

use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Violation {
    pub id: String,
    pub impact: Option<String>,
    #[serde(default)]
    pub help: String,
    #[serde(default)]
    pub nodes: usize,
}

impl Violation {
    pub fn blocks(&self) -> bool {
        matches!(self.impact.as_deref(), Some("serious" | "critical"))
    }
}

pub trait AxeRunner {
    /// Violations on the bundle's `index.html`, or why axe could not run.
    fn run(&self, bundle_dir: &Path) -> Result<Vec<Violation>, String>;
}

pub struct BunAxe {
    /// The `site/` folder.
    pub site_dir: PathBuf,
}

#[derive(Deserialize)]
struct Report {
    violations: Vec<Violation>,
}

pub fn parse_report(stdout: &str) -> Result<Vec<Violation>, String> {
    // Anything the tooling prints before the report goes to stderr; take the last JSON line to be safe.
    let line = stdout
        .lines()
        .rev()
        .find(|l| l.trim_start().starts_with('{'))
        .ok_or("axe printed no report")?;
    serde_json::from_str::<Report>(line)
        .map(|r| r.violations)
        .map_err(|e| format!("axe report is not understood: {e}"))
}

impl AxeRunner for BunAxe {
    fn run(&self, bundle_dir: &Path) -> Result<Vec<Violation>, String> {
        let script = self.site_dir.join("scripts").join("axe.ts");
        if !script.is_file() {
            return Err(format!("{} not found", script.display()));
        }
        let bun = news_win::tool_path("bun").map_err(|e| format!("cannot find bun: {e}"))?;
        let bundle = std::path::absolute(bundle_dir).map_err(|e| e.to_string())?;
        let args = [
            "scripts/axe.ts".to_owned(),
            bundle.to_string_lossy().into_owned(),
        ];
        let env = news_win::tool_env(&["BUN_INSTALL", "PLAYWRIGHT_BROWSERS_PATH"], &[]);
        let out = news_win::run_tool(&bun, &args, Some(&self.site_dir), &env)
            .map_err(|e| format!("cannot run bun: {e}"))?;
        if !out.status.success() {
            let err: String = String::from_utf8_lossy(&out.stderr)
                .chars()
                .take(300)
                .collect();
            return Err(format!("axe failed ({}): {}", out.status, err.trim()));
        }
        parse_report(&String::from_utf8_lossy(&out.stdout))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_report_and_blocks_only_serious_and_critical() {
        let v = parse_report("noise\n{\"violations\":[{\"id\":\"color-contrast\",\"impact\":\"serious\",\"help\":\"h\",\"nodes\":2},{\"id\":\"region\",\"impact\":\"moderate\",\"nodes\":1},{\"id\":\"x\",\"impact\":null}]}\n").unwrap();
        assert_eq!(v.len(), 3);
        assert_eq!(v.iter().filter(|v| v.blocks()).count(), 1);
        assert!(parse_report("").is_err());
        assert!(parse_report("{not json}").is_err());
    }
}

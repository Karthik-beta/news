//! The 11 presets as data (ADR 0008 §6.1, Bureau §8.1–§8.13). Board builds its Start and schedule forms
//! from `run.presets`; Chief runs the steps; `authorize()` reads `g_tier` and `cover`.

use news_core::NewsError;
use serde_json::{Map, Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    Code,
    AiBatch,
    Draft,
    Form,
    Approval,
}

impl StepKind {
    pub fn as_str(self) -> &'static str {
        match self {
            StepKind::Code => "code",
            StepKind::AiBatch => "ai_batch",
            StepKind::Draft => "draft",
            StepKind::Form => "form",
            StepKind::Approval => "approval",
        }
    }
    /// Steps that wait on the `ai` pool put the run in `waiting_ai`.
    pub fn is_ai(self) -> bool {
        matches!(self, StepKind::AiBatch | StepKind::Draft)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Writes {
    Pages,
    HomeCuration,
    SourceState,
    TaskDrafts,
    Jobs,
    EvalResults,
    Config,
}

impl Writes {
    pub fn as_str(self) -> &'static str {
        match self {
            Writes::Pages => "pages",
            Writes::HomeCuration => "home_curation",
            Writes::SourceState => "source_state",
            Writes::TaskDrafts => "task_drafts",
            Writes::Jobs => "jobs",
            Writes::EvalResults => "eval_results",
            Writes::Config => "config",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgType {
    Integer,
    Boolean,
    Text,
    Date,
    Enum,
    List,
}

#[derive(Debug, Clone, Copy)]
pub struct ArgDecl {
    pub name: &'static str,
    pub ty: ArgType,
    pub required: bool,
    /// JSON text.
    pub default: &'static str,
    pub doc: &'static str,
    pub choices: &'static [&'static str],
    pub min: Option<i64>,
    pub max: Option<i64>,
}

/// The G-tier commands (Bureau §6.3): they run only as the authorised step of a live run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GCommand {
    Publish,
    HomePublish,
    EvalPromote,
    CorrectionsUpdate,
    ScoresAdopt,
    SourcesToggle,
    JobsRetryBulk,
}

impl GCommand {
    pub const ALL: [GCommand; 7] = [
        GCommand::Publish,
        GCommand::HomePublish,
        GCommand::EvalPromote,
        GCommand::CorrectionsUpdate,
        GCommand::ScoresAdopt,
        GCommand::SourcesToggle,
        GCommand::JobsRetryBulk,
    ];
    /// The CLI form.
    pub fn cli(self) -> &'static str {
        match self {
            GCommand::Publish => "news publish",
            GCommand::HomePublish => "news home publish",
            GCommand::EvalPromote => "news eval promote",
            GCommand::CorrectionsUpdate => "news corrections resolve --action update",
            GCommand::ScoresAdopt => "news scores adopt",
            GCommand::SourcesToggle => "news sources enable|disable",
            GCommand::JobsRetryBulk => "news jobs retry (more than 50)",
        }
    }
}

/// What authorises a step's G-tier commands (ADR 0008 §6.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cover {
    None,
    Start,
    Gate(&'static str),
    GateOrStart(&'static str),
    PerItemGate(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub struct StepDecl {
    pub id: &'static str,
    pub title: &'static str,
    pub kind: StepKind,
    /// Bound in the binary's registry; `<preset>.<step>`.
    pub handler: &'static str,
    pub gate: Option<&'static str>,
    pub changes_to: Option<&'static str>,
    pub g_tier: &'static [GCommand],
    pub cover: Cover,
}

#[derive(Debug, Clone, Copy)]
pub struct PresetDecl {
    pub id: &'static str,
    pub name: &'static str,
    pub version: u16,
    pub heavy: bool,
    pub args: &'static [ArgDecl],
    pub writes: &'static [Writes],
    pub publishes: bool,
    pub steps: &'static [StepDecl],
    pub reference: &'static str,
}

const fn arg(
    name: &'static str,
    ty: ArgType,
    required: bool,
    default: &'static str,
    doc: &'static str,
) -> ArgDecl {
    ArgDecl {
        name,
        ty,
        required,
        default,
        doc,
        choices: &[],
        min: None,
        max: None,
    }
}

const fn int_arg(
    name: &'static str,
    default: &'static str,
    min: i64,
    max: i64,
    doc: &'static str,
) -> ArgDecl {
    ArgDecl {
        name,
        ty: ArgType::Integer,
        required: false,
        default,
        doc,
        choices: &[],
        min: Some(min),
        max: Some(max),
    }
}

const fn step(
    id: &'static str,
    title: &'static str,
    kind: StepKind,
    handler: &'static str,
) -> StepDecl {
    StepDecl {
        id,
        title,
        kind,
        handler,
        gate: None,
        changes_to: None,
        g_tier: &[],
        cover: Cover::None,
    }
}

const fn gate(
    id: &'static str,
    title: &'static str,
    handler: &'static str,
    gate: &'static str,
    changes_to: Option<&'static str>,
) -> StepDecl {
    StepDecl {
        id,
        title,
        kind: StepKind::Approval,
        handler,
        gate: Some(gate),
        changes_to,
        g_tier: &[],
        cover: Cover::None,
    }
}

const fn g(
    id: &'static str,
    title: &'static str,
    kind: StepKind,
    handler: &'static str,
    g_tier: &'static [GCommand],
    cover: Cover,
) -> StepDecl {
    StepDecl {
        id,
        title,
        kind,
        handler,
        gate: None,
        changes_to: None,
        g_tier,
        cover,
    }
}

use ArgType::*;
use GCommand as G;
use StepKind::*;

pub const CATALOGUE: [PresetDecl; 11] = [
    PresetDecl {
        id: "edition",
        name: "Edition",
        version: 1,
        heavy: true,
        args: &[
            int_arg("target", "120", 1, 1000, "Pages to aim for"),
            arg(
                "categories",
                List,
                false,
                "[]",
                "Only these categories (empty: all)",
            ),
            arg(
                "review_plan",
                Boolean,
                false,
                "false",
                "Stop for a plan approval before writing",
            ),
        ],
        writes: &[Writes::Pages],
        publishes: true,
        steps: &[
            step("preflight", "Preflight", Code, "edition.preflight"),
            step("discover", "Discover due sources", Code, "edition.discover"),
            step(
                "stories",
                "Normalise and detect stories",
                Code,
                "edition.stories",
            ),
            step("plan", "Score, select and plan", Code, "edition.plan"),
            gate(
                "review",
                "Plan approval",
                "edition.review",
                "plan",
                Some("plan"),
            ),
            step("brief", "Fetch, parse and brief", Code, "edition.brief"),
            step("write", "Write pages", AiBatch, "edition.write"),
            step(
                "render",
                "Render, check and repair",
                AiBatch,
                "edition.render",
            ),
            step(
                "spotcheck",
                "Sample spot check",
                AiBatch,
                "edition.spotcheck",
            ),
            g(
                "publish",
                "Publish",
                Code,
                "edition.publish",
                &[G::Publish],
                Cover::GateOrStart("plan"),
            ),
            // The index budget stages `index_promote` rows, a Publish-tier write: the start covers it
            // (no page is published here, only already published ones promoted).
            g(
                "promote",
                "Index promotion and prune",
                Code,
                "edition.promote",
                &[G::Publish],
                Cover::Start,
            ),
        ],
        reference: "Bureau §8.3",
    },
    PresetDecl {
        id: "flash",
        name: "Flash",
        version: 1,
        heavy: true,
        args: &[arg(
            "keys",
            List,
            true,
            "null",
            "Event keys or story ids (1 to 10)",
        )],
        writes: &[Writes::Pages],
        publishes: true,
        steps: &[
            step("preflight", "Preflight", Code, "flash.preflight"),
            step("capture", "Capture and normalise", Code, "flash.capture"),
            step("brief", "Brief", Code, "flash.brief"),
            step("write", "Write pages", AiBatch, "flash.write"),
            step(
                "render",
                "Render, check and repair",
                AiBatch,
                "flash.render",
            ),
            g(
                "publish",
                "Publish",
                Code,
                "flash.publish",
                &[G::Publish],
                Cover::Start,
            ),
        ],
        reference: "Bureau §8.4",
    },
    PresetDecl {
        id: "catchup",
        name: "Catchup",
        version: 1,
        heavy: true,
        args: &[
            arg(
                "from",
                Date,
                false,
                "null",
                "First release date (default BACKFILL_FROM)",
            ),
            arg(
                "to",
                Date,
                false,
                "null",
                "Last release date (default today)",
            ),
            arg("page_types", List, false, "[]", "Only these page types"),
            arg("sources", List, false, "[]", "Only these sources"),
            int_arg("chunk", "50", 1, 500, "Items per chunk"),
        ],
        writes: &[Writes::Pages],
        publishes: true,
        steps: &[
            step("preflight", "Preflight", Code, "catchup.preflight"),
            step(
                "enumerate",
                "Enumerate historical events",
                Code,
                "catchup.enumerate",
            ),
            step("select", "Score and select", Code, "catchup.select"),
            gate("review", "Plan approval", "catchup.review", "plan", None),
            g(
                "chunks",
                "Write and publish by chunk",
                AiBatch,
                "catchup.chunks",
                &[G::Publish],
                Cover::Gate("plan"),
            ),
        ],
        reference: "Bureau §8.5",
    },
    PresetDecl {
        id: "frontpage",
        name: "Frontpage",
        version: 1,
        heavy: false,
        args: &[],
        writes: &[Writes::HomeCuration],
        publishes: true,
        steps: &[
            step(
                "candidates",
                "Home candidates",
                Code,
                "frontpage.candidates",
            ),
            step("draft", "Curator draft", Draft, "frontpage.draft"),
            step("check", "Save and check", Code, "frontpage.check"),
            step("preview", "Preview", Code, "frontpage.preview"),
            gate(
                "review",
                "Curation approval",
                "frontpage.review",
                "publish",
                Some("draft"),
            ),
            g(
                "publish",
                "Publish the curation",
                Code,
                "frontpage.publish",
                &[G::HomePublish],
                Cover::Gate("publish"),
            ),
        ],
        reference: "Bureau §8.6",
    },
    PresetDecl {
        id: "rollcall",
        name: "Rollcall",
        version: 1,
        heavy: false,
        args: &[arg(
            "source",
            Text,
            false,
            "null",
            "Only this source (default: every red or amber one)",
        )],
        writes: &[Writes::SourceState, Writes::TaskDrafts],
        publishes: false,
        steps: &[
            step("health", "Source health", Code, "rollcall.health"),
            step(
                "probe",
                "Probe red and amber sources",
                Code,
                "rollcall.probe",
            ),
            step("proposals", "Proposals draft", Draft, "rollcall.proposals"),
            gate(
                "review",
                "Actions approval",
                "rollcall.review",
                "actions",
                Some("proposals"),
            ),
            g(
                "apply",
                "Apply approved actions",
                Code,
                "rollcall.apply",
                &[G::SourcesToggle],
                Cover::Gate("actions"),
            ),
        ],
        reference: "Bureau §8.7",
    },
    PresetDecl {
        id: "salvage",
        name: "Salvage",
        version: 1,
        heavy: true,
        args: &[
            arg("class", Text, false, "null", "Only this error class"),
            arg("since", Date, false, "null", "Only jobs that failed since"),
        ],
        writes: &[Writes::Jobs, Writes::Pages],
        publishes: true,
        steps: &[
            step("dead", "Group dead jobs", Code, "salvage.dead"),
            gate(
                "requeue_review",
                "Requeue approval (above 50 jobs)",
                "salvage.requeue_review",
                "requeue",
                None,
            ),
            g(
                "retry",
                "Retry transient and rate-limited",
                Code,
                "salvage.retry",
                &[G::JobsRetryBulk],
                Cover::GateOrStart("requeue"),
            ),
            step(
                "revalidate",
                "Re-validate stored outputs",
                Code,
                "salvage.revalidate",
            ),
            gate(
                "reexecute_review",
                "Re-execute approval",
                "salvage.reexecute_review",
                "reexecute",
                None,
            ),
            step("rerun", "Re-run AI", AiBatch, "salvage.rerun"),
            g(
                "publish",
                "Render, check and publish",
                Code,
                "salvage.publish",
                &[G::Publish],
                Cover::Gate("reexecute"),
            ),
        ],
        reference: "Bureau §8.8",
    },
    PresetDecl {
        id: "followup",
        name: "Followup",
        version: 1,
        heavy: true,
        args: &[arg(
            "source",
            Text,
            false,
            "null",
            "Only revisions from this source",
        )],
        writes: &[Writes::Pages],
        publishes: true,
        steps: &[
            step("find", "Find superseded data points", Code, "followup.find"),
            step("classify", "Classify revisions", Code, "followup.classify"),
            gate(
                "bulk_review",
                "Bulk approval (above 100 pages)",
                "followup.bulk_review",
                "bulk",
                None,
            ),
            step(
                "rerender",
                "Re-render value-only pages",
                Code,
                "followup.rerender",
            ),
            step(
                "update",
                "Update direction flips",
                AiBatch,
                "followup.update",
            ),
            g(
                "publish",
                "Check and publish",
                Code,
                "followup.publish",
                &[G::Publish],
                Cover::GateOrStart("bulk"),
            ),
        ],
        reference: "Bureau §8.9",
    },
    PresetDecl {
        id: "errata",
        name: "Errata",
        version: 1,
        heavy: false,
        args: &[],
        writes: &[Writes::Pages],
        publishes: true,
        steps: &[
            step("pull", "Pull reports", Code, "errata.pull"),
            step("triage", "Triage", AiBatch, "errata.triage"),
            step("verify", "Verify figures", Code, "errata.verify"),
            gate(
                "review",
                "Approval per report",
                "errata.review",
                "report",
                None,
            ),
            step("fix", "Fix", AiBatch, "errata.fix"),
            g(
                "publish",
                "Publish and resolve",
                Code,
                "errata.publish",
                &[G::Publish, G::CorrectionsUpdate],
                Cover::PerItemGate("report"),
            ),
        ],
        reference: "Bureau §8.10",
    },
    PresetDecl {
        id: "audit",
        name: "Audit",
        version: 1,
        heavy: true,
        args: &[
            ArgDecl {
                name: "suite",
                ty: Enum,
                required: true,
                default: "null",
                doc: "The evaluation suite",
                choices: &[
                    "writer",
                    "classify",
                    "storymatch",
                    "spotcheck",
                    "curator",
                    "all",
                ],
                min: None,
                max: None,
            },
            arg(
                "prompt_version",
                Text,
                true,
                "null",
                "The candidate prompt, skill or model version",
            ),
        ],
        writes: &[Writes::EvalResults, Writes::Config],
        publishes: false,
        steps: &[
            step("load", "Load the golden set", Code, "audit.load"),
            step("metrics", "Deterministic metrics", Code, "audit.metrics"),
            step("candidate", "Candidate calls", AiBatch, "audit.candidate"),
            step("judges", "Judges on a sample", AiBatch, "audit.judges"),
            step("report", "Comparison report", Code, "audit.report"),
            gate(
                "review",
                "Promote approval",
                "audit.review",
                "promote",
                None,
            ),
            g(
                "promote",
                "Promote",
                Code,
                "audit.promote",
                &[G::EvalPromote],
                Cover::Gate("promote"),
            ),
        ],
        reference: "Bureau §8.11",
    },
    PresetDecl {
        id: "huddle",
        name: "Huddle",
        version: 1,
        heavy: false,
        args: &[],
        writes: &[Writes::Config],
        publishes: false,
        steps: &[
            step("debrief", "Debrief", Code, "huddle.debrief"),
            step(
                "calibration",
                "Ranking calibration",
                Form,
                "huddle.calibration",
            ),
            step("propose", "Propose changes", Code, "huddle.propose"),
            gate(
                "review",
                "Approval per change",
                "huddle.review",
                "change",
                None,
            ),
            g(
                "apply",
                "Apply approved changes",
                Code,
                "huddle.apply",
                &[G::ScoresAdopt],
                Cover::PerItemGate("change"),
            ),
        ],
        reference: "Bureau §8.12",
    },
    PresetDecl {
        id: "reprint",
        name: "Reprint",
        version: 1,
        heavy: true,
        args: &[
            arg(
                "renderer",
                Text,
                true,
                "null",
                "Re-render pages older than this renderer version",
            ),
            arg("page_types", List, false, "[]", "Only these page types"),
            int_arg("limit", "200", 1, 10_000, "At most this many pages"),
        ],
        writes: &[Writes::Pages],
        publishes: true,
        steps: &[
            step("select", "Select pages", Code, "reprint.select"),
            step("rerender", "Re-render", Code, "reprint.rerender"),
            step("check", "Checks", Code, "reprint.check"),
            gate(
                "review",
                "Sample approval",
                "reprint.review",
                "sample",
                None,
            ),
            g(
                "publish",
                "Paced publish",
                Code,
                "reprint.publish",
                &[G::Publish],
                Cover::Gate("sample"),
            ),
        ],
        reference: "Bureau §8.13",
    },
];

/// A preset by id.
pub fn preset(id: &str) -> Option<&'static PresetDecl> {
    CATALOGUE.iter().find(|p| p.id == id)
}

impl PresetDecl {
    pub fn step(&self, id: &str) -> Option<(i16, &'static StepDecl)> {
        self.steps
            .iter()
            .enumerate()
            .find(|(_, s)| s.id == id)
            .map(|(i, s)| (i as i16 + 1, s))
    }

    pub fn step_at(&self, seq: i16) -> Option<&'static StepDecl> {
        usize::try_from(seq - 1)
            .ok()
            .and_then(|i| self.steps.get(i))
    }

    /// Gate names; per-item families as `report:*`.
    pub fn gates(&self) -> Vec<String> {
        self.steps
            .iter()
            .filter_map(|s| s.gate)
            .map(
                |g| match self.steps.iter().any(|s| s.cover == Cover::PerItemGate(g)) {
                    true => format!("{g}:*"),
                    false => g.to_owned(),
                },
            )
            .collect()
    }

    /// Check `args` and fill in defaults (`validation_failed` on unknown names, missing required ones
    /// or wrong types).
    pub fn resolve_args(&self, args: &Map<String, Value>) -> Result<Map<String, Value>, NewsError> {
        for k in args.keys() {
            if !self.args.iter().any(|a| a.name == k) {
                return Err(NewsError::validation(format!(
                    "{} has no argument {k:?}",
                    self.name
                )));
            }
        }
        let mut out = Map::new();
        for a in self.args {
            let v = match args.get(a.name) {
                Some(v) if !v.is_null() => v.clone(),
                _ if a.required => {
                    return Err(NewsError::validation(format!(
                        "{} needs the argument {:?}",
                        self.name, a.name
                    )));
                }
                _ => serde_json::from_str(a.default).unwrap_or(Value::Null),
            };
            let ok = v.is_null()
                || match a.ty {
                    ArgType::Integer => v.as_i64().is_some_and(|n| {
                        a.min.is_none_or(|m| n >= m) && a.max.is_none_or(|m| n <= m)
                    }),
                    ArgType::Boolean => v.is_boolean(),
                    ArgType::Text => v.as_str().is_some_and(|s| !s.is_empty()),
                    ArgType::Date => v
                        .as_str()
                        .is_some_and(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()),
                    ArgType::Enum => v.as_str().is_some_and(|s| a.choices.contains(&s)),
                    ArgType::List => v
                        .as_array()
                        .is_some_and(|l| l.iter().all(|x| x.is_string() || x.is_i64())),
                };
            if !ok {
                return Err(NewsError::validation(format!(
                    "{}: argument {:?} has the wrong type or range ({v})",
                    self.name, a.name
                )));
            }
            out.insert(a.name.to_owned(), v);
        }
        Ok(out)
    }

    /// The `run.presets` item (contracts/hotline/common.schema.json `presetDecl`).
    pub fn to_json(&self) -> Value {
        let ty = |t: ArgType| match t {
            ArgType::Integer => "integer",
            ArgType::Boolean => "boolean",
            ArgType::Text => "text",
            ArgType::Date => "date",
            ArgType::Enum => "enum",
            ArgType::List => "list",
        };
        json!({
            "id": self.id,
            "name": self.name,
            "heavy": self.heavy,
            "writes": self.writes.iter().map(|w| w.as_str()).collect::<Vec<_>>(),
            "publishes": self.publishes,
            "gates": self.gates(),
            "args": self.args.iter().map(|a| json!({
                "name": a.name,
                "type": ty(a.ty),
                "required": a.required,
                "default": serde_json::from_str::<Value>(a.default).unwrap_or(Value::Null),
                "doc": a.doc,
                "choices": if a.choices.is_empty() { Value::Null } else { json!(a.choices) },
                "min": a.min,
                "max": a.max,
            })).collect::<Vec<_>>(),
            "steps": self.steps.iter().enumerate().map(|(i, s)| json!({
                "seq": i + 1,
                "id": s.id,
                "title": s.title,
                "kind": s.kind.as_str(),
                "gate": s.gate,
            })).collect::<Vec<_>>(),
            "reference": self.reference,
        })
    }
}

/// The `run.presets` data.
pub fn catalogue_json() -> Value {
    json!({ "items": CATALOGUE.iter().map(PresetDecl::to_json).collect::<Vec<_>>() })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_is_well_formed() {
        let mut ids: Vec<&str> = CATALOGUE.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 11);
        for p in &CATALOGUE {
            assert!(p.id.bytes().all(|b| b.is_ascii_lowercase()), "{}", p.id);
            let mut steps: Vec<&str> = p.steps.iter().map(|s| s.id).collect();
            steps.sort_unstable();
            steps.dedup();
            assert_eq!(steps.len(), p.steps.len(), "{}: step ids unique", p.id);
            for (i, s) in p.steps.iter().enumerate() {
                assert_eq!(s.handler, format!("{}.{}", p.id, s.id), "{}", s.handler);
                assert!(
                    s.id.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
                    "{}",
                    s.id
                );
                // Every G step has a cover, and every gate it names is an earlier approval step.
                if !s.g_tier.is_empty() {
                    assert_ne!(s.cover, Cover::None, "{}", s.handler);
                }
                if let Cover::Gate(gn) | Cover::GateOrStart(gn) | Cover::PerItemGate(gn) = s.cover {
                    assert!(
                        p.steps[..i]
                            .iter()
                            .any(|e| e.kind == StepKind::Approval && e.gate == Some(gn)),
                        "{}: gate {gn} is not an earlier approval step",
                        s.handler
                    );
                }
                assert_eq!(
                    s.kind == StepKind::Approval,
                    s.gate.is_some(),
                    "{}",
                    s.handler
                );
                if let Some(c) = s.changes_to {
                    assert!(
                        p.steps[..i].iter().any(|e| e.id == c),
                        "{}: changes_to {c}",
                        s.handler
                    );
                }
            }
            assert_eq!(
                p.publishes,
                p.steps.iter().any(|s| s
                    .g_tier
                    .iter()
                    .any(|g| matches!(g, G::Publish | G::HomePublish))),
                "{}",
                p.id
            );
        }
    }

    #[test]
    fn heavy_matches_bureau_8_1() {
        let heavy: Vec<&str> = CATALOGUE.iter().filter(|p| p.heavy).map(|p| p.id).collect();
        assert_eq!(
            heavy,
            [
                "edition", "flash", "catchup", "salvage", "followup", "audit", "reprint"
            ]
        );
    }

    #[test]
    fn args_are_checked_and_defaulted() {
        let e = preset("edition").unwrap_or_else(|| panic!("edition"));
        let out = e
            .resolve_args(&Map::new())
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(out["target"], 120);
        assert_eq!(out["review_plan"], false);
        let bad = json!({"target": 0});
        assert!(
            e.resolve_args(bad.as_object().unwrap_or(&Map::new()))
                .is_err()
        );
        let unknown = json!({"nope": 1});
        assert!(
            e.resolve_args(unknown.as_object().unwrap_or(&Map::new()))
                .is_err()
        );
        let audit = preset("audit").unwrap_or_else(|| panic!("audit"));
        assert!(
            audit.resolve_args(&Map::new()).is_err(),
            "suite is required"
        );
    }
}

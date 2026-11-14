---
name: news-subagent-workflow
description: "How the orchestrator session plans, briefs, runs and merges work through Opus 5.5 and Sonnet 5.5 sub-agents in the News repo: model choice per task, brief template, worktrees and branch names, review loop, PRs with CodeRabbit, parallelism limits. Use this whenever you are about to delegate any development task, start several agents, open or merge a PR, or move a phase of the Development Plan forward."
---

# Running development through sub-agents

The source of truth is `News — Development Plan.md`. This skill is the operating procedure for it. The orchestrator (the main session, Opus 5.5) coordinates, briefs and integrates; it does not write large amounts of code itself and never pulls whole files or logs into its own context.

## Pick the model

| Work | Model |
|---|---|
| Architecture, ADRs, data models, interfaces | Opus 5.5 |
| Spikes and unknowns (OpenUI SSR, MCP v2, caching) | Opus 5.5 |
| **Components and charts: building** | Opus 5.5 |
| **Components and charts: testing** | **Opus 5.5 only**, a different agent from the builder |
| Bug fixes (anything beyond a one-line fix) | Opus 5.5 |
| Reviews, security reviews | Opus 5.5 |
| Skills, prompts, reader-facing prose | Opus 5.5 |
| Correctness-critical parsing (XBRL, units, currencies, fiscal periods) | Opus 5.5 |
| Well-specified features: connectors, CLI commands, Workers, templates, scripts, dashboard | Sonnet 5.5 |
| Tests for non-component code | Sonnet 5.5 |
| Docs drafts | Sonnet 5.5 (Opus reviews anything readers see) |

Effort: high for every role. When unsure, ask: does this need judgement (Opus) or volume on a clear spec (Sonnet)?

## Brief template

Briefs are the agent's only context. Point at files; don't paste them.

```
Goal: <one sentence, the outcome>
Phase / task: <Development Plan phase and checklist item>
Read first: <2–6 files, e.g. design/components.md#key-figures-keyfigures, site/src/components/FigureFrame/>
Skills to load: <news-design-system, news-component-builder, …>
Branch: phase-<N>/<area>/<short-name>   Worktree: yes
Interfaces to honour: <types, CLI JSON shapes, data-reference contract>
Out of scope: <what not to touch>
Done when: <tests, behaviours, files>
Return: branch, ≤ 5-line summary, test results, open questions. No file contents, no logs.
```

## Run it

1. **Foundations first.** Shared pieces (tokens, primitives, core crates) are built by one agent and merged before any fan-out.
2. **Isolation.** Each builder works in its own git worktree (Agent tool `isolation: "worktree"`) on its own branch. No two agents touch the same files.
3. **Limits.** At most 3 coding agents at once, plus up to 2 read-only agents. One heavy job at a time (full Playwright matrix, Rust release builds, Docling). Never start work that depends on an unmerged branch.
4. **Review.** When a builder returns, start a fresh Opus reviewer with the `news-review` skill on that branch. At most two fix rounds; then hand it to the Opus bug fixer with the reviewer's notes.
5. **Components** also go to an Opus tester (`news-component-testing`) before the PR. No sign-off, no PR.
6. **PR.** `gh pr create --base main --head <branch>` with a summary, the checklist item, test evidence and screenshots for visual changes. Then bind the PR and let CI and CodeRabbit (light profile) run.
7. **CodeRabbit comments.** Fix the real ones on the same branch (builder or bug fixer); reply to the rest with the reason. Don't argue with style nits; the profile should keep them rare.
8. **Merge.** Squash-merge when CI is green and both reviews are done. Karthik merges phase-exit PRs and anything touching secrets, the write API or the brand. Never force-push `main`; never skip hooks.
9. **Update the plan.** Tick the item in the Development Plan's progress checklist in the same or the next PR.

## Token discipline

- Sub-agents return summaries; ask for a path when you need detail, then read only that part.
- A wide search goes to one read-only explorer that reports conclusions.
- Opus where it pays, Sonnet for volume. If a plan usage limit is near: let in-flight agents finish, start nothing new, and tell Karthik.

## Things only Karthik does

Create cloud accounts, handle credentials and keys, set DNS, approve brand-level design changes, approve phase exits, approve new paid services (there should be none).

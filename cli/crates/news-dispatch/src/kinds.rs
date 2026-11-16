//! The job-kind registry (§5.4): every kind, its default pool and lease, and the subsystem whose
//! handler runs it. The `news` binary binds `owner` to that subsystem crate's handler.

/// One job kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KindSpec {
    pub kind: &'static str,
    pub pool: &'static str,
    /// Default lease: 2 minutes for code jobs, 10 for AI jobs (§5.4).
    pub lease_secs: u32,
    /// Subsystem crate that handles it (`news-<owner>`).
    pub owner: &'static str,
}

const CODE: u32 = 120;
const AI: u32 = 600;

const fn k(
    kind: &'static str,
    pool: &'static str,
    lease_secs: u32,
    owner: &'static str,
) -> KindSpec {
    KindSpec {
        kind,
        pool,
        lease_secs,
        owner,
    }
}

/// Every kind named in §5.4, §5.6 and §8.14, sorted by kind.
pub const KINDS: [KindSpec; 22] = [
    k("ai.classify", "ai", AI, "ghost"),
    k("ai.condense", "ai", AI, "ghost"),
    k("ai.repair", "ai", AI, "ghost"),
    k("ai.spotcheck", "ai", AI, "ghost"),
    k("ai.storymatch", "ai", AI, "ghost"),
    k("ai.write", "ai", AI, "ghost"),
    k("calendar.capture", "net", CODE, "diary"),
    k("calendar.sync", "net", CODE, "diary"),
    k("doc.fetch", "net", CODE, "wire"),
    k("doc.parse", "cpu", CODE, "clerk"),
    k("doc.parse_docling", "docling", CODE * 5, "clerk"),
    k("index.promote", "publish", CODE, "press"),
    k("item.brief", "cpu", CODE, "fixer"),
    k("item.check", "cpu", CODE, "proof"),
    k("item.render", "render", CODE, "proof"),
    k("page.publish", "publish", CODE, "press"),
    k("page.revise", "cpu", CODE, "rewrite"),
    k("run.select", "cpu", CODE, "nose"),
    k("source.discover", "net", CODE, "wire"),
    k("story.detect", "cpu", CODE, "taster"),
    k("story.score", "cpu", CODE, "nose"),
    k("work.prune", "cpu", CODE, "wire"),
];

/// Kinds added for schedules beyond §5.4: the analytics pull (§8.14), owned by Ticker (`news-telemetry`,
/// #16) per the TD's telemetry section (cloud analytics pulled through their APIs).
pub const EXTRA_KINDS: [KindSpec; 1] = [k("traffic.pull", "net", CODE, "telemetry")];

pub fn spec(kind: &str) -> Option<&'static KindSpec> {
    KINDS
        .iter()
        .chain(EXTRA_KINDS.iter())
        .find(|s| s.kind == kind)
}

pub fn all() -> impl Iterator<Item = &'static KindSpec> {
    KINDS.iter().chain(EXTRA_KINDS.iter())
}

/// The lease a claim in `pool` takes: the longest default lease of the pool's kinds (2 minutes when
/// none). Running jobs then heartbeat with their own kind's lease.
pub fn pool_lease(pool: &str) -> std::time::Duration {
    let secs = all()
        .filter(|s| s.pool == pool)
        .map(|s| s.lease_secs)
        .max()
        .unwrap_or(CODE);
    std::time::Duration::from_secs(u64::from(secs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::POOLS;

    #[test]
    fn kinds_are_sorted_unique_and_in_known_pools() {
        assert!(KINDS.windows(2).all(|w| w[0].kind < w[1].kind));
        for s in all() {
            assert!(POOLS.contains(&s.pool), "{}", s.kind);
            assert!(s.lease_secs >= 60, "{}", s.kind);
            assert_eq!(spec(s.kind), Some(s));
        }
    }

    #[test]
    fn ai_kinds_run_only_in_the_ai_pool_with_long_leases() {
        for s in all() {
            assert_eq!(s.kind.starts_with("ai."), s.pool == "ai", "{}", s.kind);
            if s.pool == "ai" {
                assert_eq!(s.lease_secs, 600);
            }
        }
    }

    #[test]
    fn publishing_kinds_never_run_in_work_pools() {
        for s in all().filter(|s| s.owner == "press") {
            assert!(!crate::BACKGROUND_POOLS.contains(&s.pool), "{}", s.kind);
        }
    }
}

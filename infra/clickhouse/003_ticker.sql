-- Run as the news user by scripts/clickhouse-setup.ts (one statement per request). Idempotent.
-- Ticker (Bureau Design §9.2): columns that are filtered and grouped on, and the per-call tables.

ALTER TABLE news.events
    ADD COLUMN IF NOT EXISTS job_id         String DEFAULT '',
    ADD COLUMN IF NOT EXISTS attempt        UInt8 DEFAULT 0,
    ADD COLUMN IF NOT EXISTS preset         LowCardinality(String) DEFAULT '',
    ADD COLUMN IF NOT EXISTS step           LowCardinality(String) DEFAULT '',
    ADD COLUMN IF NOT EXISTS source_id      LowCardinality(String) DEFAULT '',
    ADD COLUMN IF NOT EXISTS error_class    LowCardinality(String) DEFAULT '',
    ADD COLUMN IF NOT EXISTS prompt_version LowCardinality(String) DEFAULT '',
    ADD COLUMN IF NOT EXISTS trace_id       String DEFAULT '',
    ADD COLUMN IF NOT EXISTS span_id        String DEFAULT '';

-- One row per `claude -p` call (Ghost).
CREATE TABLE IF NOT EXISTS news.ai_calls
(
    ts              DateTime64(3, 'UTC'),
    run_id          String,
    job_id          String,
    step            LowCardinality(String),   -- A1 … A12
    model           LowCardinality(String),
    prompt_version  LowCardinality(String),
    tokens_in       UInt32,
    tokens_out      UInt32,
    cache_read      UInt32,
    cache_write_5m  UInt32,
    cache_write_1h  UInt32,
    turns           UInt8,
    duration_ms     UInt32,
    subtype         LowCardinality(String),   -- success, error_max_structured_output_retries, …
    validation      LowCardinality(String),   -- valid, invalid:<rule>
    repair_round    UInt8,
    est_cost_usd    Float64
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(ts)
ORDER BY (step, ts)
TTL toDateTime(ts) + INTERVAL 2 YEAR;

-- One row per HTTP request (the Runner).
CREATE TABLE IF NOT EXISTS news.fetches
(
    ts            DateTime64(3, 'UTC'),
    run_id        String,
    job_id        String,
    source_id     LowCardinality(String),
    host          LowCardinality(String),
    status        UInt16,
    not_modified  UInt8,                      -- 1 for a 304
    bytes         UInt64,
    duration_ms   UInt32,
    retry_after_s UInt32
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(ts)
ORDER BY (source_id, ts)
TTL toDateTime(ts) + INTERVAL 2 YEAR;

-- One row per score computation (Nose).
CREATE TABLE IF NOT EXISTS news.scores
(
    ts             DateTime64(3, 'UTC'),
    run_id         String,
    story_id       UInt64,
    scorer_version LowCardinality(String),
    components     String CODEC(ZSTD(3)),     -- JSON: {"I":…, "M":…, …}
    total          Float64,
    selected       UInt8,
    tier           UInt8
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(ts)
ORDER BY (scorer_version, ts)
TTL toDateTime(ts) + INTERVAL 2 YEAR;

-- One row per check per page (Proof).
CREATE TABLE IF NOT EXISTS news.check_results
(
    ts        DateTime64(3, 'UTC'),
    run_id    String,
    page_id   String,
    check_id  LowCardinality(String),         -- C1 … C14, A7
    pass      UInt8,
    message   String
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(ts)
ORDER BY (check_id, ts)
TTL toDateTime(ts) + INTERVAL 2 YEAR;

-- One row per run, replaced as the run progresses (Chief).
CREATE TABLE IF NOT EXISTS news.runs
(
    updated_at   DateTime64(3, 'UTC'),
    run_id       String,
    short_id     String,
    preset       LowCardinality(String),
    status       LowCardinality(String),
    counts       String CODEC(ZSTD(3)),       -- JSON
    budget_used  String CODEC(ZSTD(3)),       -- JSON
    started_at   DateTime64(3, 'UTC'),
    duration_ms  UInt64
)
ENGINE = ReplacingMergeTree(updated_at)
PARTITION BY toYYYYMM(started_at)
ORDER BY run_id;

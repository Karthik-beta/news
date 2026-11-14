-- Run as the news user. The core pipeline events table (Technical Design, "Core table: events").
-- OTel tables (otel_logs, otel_metrics_*, otel_traces) are created by the collector's ClickHouse exporter.
CREATE TABLE IF NOT EXISTS news.events
(
    ts           DateTime64(3, 'UTC'),
    run_id       String,
    item_id      String,
    page_id      String,
    stage        LowCardinality(String),
    event        LowCardinality(String),
    model        LowCardinality(String),
    tokens_in    UInt32,
    tokens_out   UInt32,
    cache_read   UInt32,
    cache_write  UInt32,
    duration_ms  UInt32,
    status       LowCardinality(String),
    error        String,
    attrs        String CODEC(ZSTD(3))   -- JSON: page type, tier, category, region, …
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(ts)
ORDER BY (stage, event, ts)
TTL toDateTime(ts) + INTERVAL 2 YEAR;

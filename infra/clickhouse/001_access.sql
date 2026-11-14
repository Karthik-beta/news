-- Run as the shared container's admin user by scripts/clickhouse-setup.ts.
-- Isolates News inside the shared ClickHouse: its own database, user, settings profile and quota.
-- {{NEWS_CH_PASSWORD}} is substituted from .env at run time; this file never holds a secret.

CREATE DATABASE IF NOT EXISTS news;

CREATE SETTINGS PROFILE IF NOT EXISTS news_profile SETTINGS
    max_threads = 2,
    max_memory_usage = 2000000000,
    max_execution_time = 300,
    max_insert_threads = 1;

CREATE USER IF NOT EXISTS news IDENTIFIED WITH sha256_password BY '{{NEWS_CH_PASSWORD}}'
    DEFAULT DATABASE news
    SETTINGS PROFILE 'news_profile';

-- Keep the password in step with .env when setup is re-run.
ALTER USER news IDENTIFIED WITH sha256_password BY '{{NEWS_CH_PASSWORD}}';

GRANT SELECT, INSERT, ALTER, CREATE TABLE, CREATE VIEW, CREATE DATABASE, DROP TABLE, DROP VIEW, TRUNCATE, OPTIMIZE, SHOW ON news.* TO news;

CREATE QUOTA IF NOT EXISTS news_quota
    FOR INTERVAL 1 hour MAX queries = 50000, errors = 5000, execution_time = 3600
    TO news;

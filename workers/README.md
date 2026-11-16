# Workers

Cloudflare Workers for the cloud side of News. The cloud stores and serves; it runs no AI (ADR 0006).

| Package | What |
| --- | --- |
| `api/` | Write API: page publish, withdraw, index promotion, home curation and rollback, taxonomy, redirects, reports |
| `mcp/` | Read-only MCP server (spec revision 2026-07-28) |
| `search/` | Public search API (placeholder until #14) |
| `shared/` | DynamoDB client and fake, read layer, D1 search, R2, purge, precompiled validators, curation checks |
| `test-support/` | Vitest configs, DynamoDB Local helper, Wrangler E2E launcher |

Design: ADR 0004 (data layer), 0005 (home curation), 0006 (security). Contracts: `contracts/`.

## api routes

| Route | Auth | Does |
| --- | --- | --- |
| `GET /health` | none | Liveness |
| `PUT /v1/pages/{id}` | key + Idempotency-Key | Publish a bundle (at most 1 MiB): R2 files, `PAGE#`, `HUB#`, `ENT#`, D1 row, purge |
| `POST /v1/pages/{id}/withdraw` | key + Idempotency-Key | Withdraw: record kept, hubs, search row and public files removed (a retry finishes a partial one) |
| `POST /v1/pages/{id}/index` | key + Idempotency-Key | Set `index_status` |
| `PUT /v1/home/curation` | key + Idempotency-Key | Re-check, write `V#<n>`, move `LATEST`, purge home and region views |
| `POST /v1/home/curation/rollback` | key + Idempotency-Key | Make an earlier version live |
| `PUT /v1/taxonomy`, `PUT /v1/redirects` | key + Idempotency-Key | Category nodes, redirects |
| `GET /v1/reports` | key | Reader reports for Errata (from the `REPORTS` D1 database) |
| `POST /v1/reports`, `OPTIONS /v1/reports` | none (rate-limited, daily cap) | Public report intake |

Full contract: `contracts/cloud/write-api.openapi.json`. Everything under `/v1/` except the report intake is authenticated before routing. Publish, withdraw and index promotion of one page run one at a time (page lock; a second gets 409 `page_busy`), and a retry with the same Idempotency-Key finishes whatever a failed attempt left undone (ADR 0004).

## mcp

`POST /mcp`; `GET /health`. Methods: `server/discover`, `tools/list`, `tools/call`, `ping`, and `initialize` for clients of 2025-11-25 / 2025-06-18. Tools: `search_pages`, `get_page`, `list_categories`, `get_entity`, `get_data`, all read-only. Eventually consistent DynamoDB reads, results cached at the edge for 60 seconds (category tree 5 minutes), 30 requests a minute per client, CORS for allowed browser origins.

## Bindings

| Binding | api | mcp | Kind | Set by |
| --- | --- | --- | --- | --- |
| `PAGES` | yes | yes | R2 bucket | Karthik (#21), `wrangler.jsonc` |
| `SEARCH_<YYYY>` | yes | yes (SELECT only) | D1, one per year; migrations in `shared/migrations/search` | Karthik (#21) |
| `REPORTS` | yes | no | D1 `news-reports`; migrations in `api/migrations/reports` | Karthik (#21) |
| `NEWS_DYNAMODB_TABLE`, `NEWS_DYNAMODB_REGION` | yes | yes | vars | Karthik (#21) |
| `NEWS_DYNAMODB_ACCESS_KEY_ID`, `NEWS_DYNAMODB_SECRET_ACCESS_KEY` | publish-only key | read-only key | secrets | Karthik, `wrangler secret put` |
| `NEWS_WRITE_API_KEY` | yes | no | secret, 32+ characters | Karthik |
| `CLOUDFLARE_ZONE_ID` | yes | no | var | Karthik |
| `CLOUDFLARE_PURGE_TOKEN` | yes | no | secret, Zone > Cache Purge only | Karthik |
| `NEWS_SITE_ORIGIN` | yes | yes | var (in `wrangler.jsonc`) | done |
| `NEWS_REGION_VIEWS` | optional | no | var, comma-separated | optional |
| `NEWS_REPORT_DAILY_CAP` | optional | no | var, reports per UTC day site-wide (default 200) | optional |
| `NEWS_MCP_ALLOWED_ORIGINS` | no | optional | var | optional |
| `AUTH_FAILURE_LIMITER`, `REPORT_IP_LIMITER`, `REPORT_GLOBAL_LIMITER` | yes | no | rate limit | done |
| `MCP_RATE_LIMITER` | no | yes | rate limit | done |

Without a zone and purge token the api logs `purge_skipped` instead of purging (local runs); a malformed zone ID fails only purging requests and is logged once. Each `wrangler.jsonc` keeps its `TODO(` until the real bindings are in, which keeps `deploy.yml` from deploying it.

## Least-privilege IAM policies (#21)

Two IAM users, one per Worker, each with only its policy. Replace `<region>`, `<account>` and `<table>`. TTL on `expires_at` is switched on once by Karthik with an admin login, not by these keys.

api Worker (publish key): items of the one table only; no `Scan`, `UpdateItem` or table management. Transactions need the actions of the items inside them.

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "NewsWriteApiItems",
      "Effect": "Allow",
      "Action": [
        "dynamodb:GetItem",
        "dynamodb:BatchGetItem",
        "dynamodb:Query",
        "dynamodb:PutItem",
        "dynamodb:DeleteItem",
        "dynamodb:BatchWriteItem",
        "dynamodb:ConditionCheckItem"
      ],
      "Resource": "arn:aws:dynamodb:<region>:<account>:table/<table>"
    }
  ]
}
```

MCP Worker (read-only key): reads, and only from the public partitions, so it cannot read idempotency records or curations.

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "NewsMcpPublicReads",
      "Effect": "Allow",
      "Action": ["dynamodb:GetItem", "dynamodb:BatchGetItem", "dynamodb:Query"],
      "Resource": "arn:aws:dynamodb:<region>:<account>:table/<table>",
      "Condition": {
        "ForAllValues:StringLike": {
          "dynamodb:LeadingKeys": ["PAGE#*", "HUB#*", "ENT#*", "TAX#tree", "URL#*"]
        }
      }
    }
  ]
}
```

The site's SSR Worker (#14) will need its own read-only key like the MCP one, plus `HOME#CURATION`.

## Deployment steps that only Karthik can do (#21)

- Create the `REPORTS` D1 database and declare it in `api/wrangler.jsonc` (the TODO there shows how), then apply `api/migrations/reports`.
- Add a WAF rate-limiting rule on `/mcp` (the free plan has one): the Workers rate limits are per location, so this is the only global budget for MCP reads of the shared table (ADR 0006).
- After the first deploy, check one real Workers log line for each Worker: invocation logs are off, and no IP address or user agent should appear.

## Local testing

From the repo root: `bun run infra:up` once (DynamoDB Local on 127.0.0.1:8000), then

- `bun --filter './workers/*' test`: unit tests. `shared` runs in Node; `api` and `mcp` run inside workerd with in-memory fakes (`FakeTable`, `FakeBucket`, recording search and purge).
- `bun --filter './workers/*' test:integration`: inside workerd with Miniflare's local R2 and D1 (the real search schema) and DynamoDB Local through the real SigV4 client. The same table contract suite runs against the fake and DynamoDB Local.
- `bun --filter './workers/*' test:e2e`: each Worker started by Wrangler from its real `wrangler.jsonc`, over HTTP.

After changing a schema in `contracts/`, run `bun run validators` in `workers/shared`.

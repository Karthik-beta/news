# Cloud provisioning and deployment

Code for the cloud half of issue #21. Nothing here has been run against a real account: the first run is Karthik's, with his own credentials (see "What only Karthik can do").

## What it provisions

`provision.ts` makes sure each of these exists and is right. Run it twice: the second run changes nothing.

| Group | Resource | Notes |
| --- | --- | --- |
| `r2` | Bucket `news-pages`, custom domain `news.karthik.nz` on it | Article pages are served from R2 through the CDN (ADR 0010) |
| `d1` | `news-search-<year>` for each year in `NEWS_SEARCH_YEARS`, and `news-reports` | Migrations from `workers/shared/migrations/search` and `workers/api/migrations/reports`, recorded in `d1_migrations` (Wrangler's own table, so `wrangler d1 migrations` agrees) |
| `dynamodb` | Table `news` (keys `PK`, `SK`), provisioned 25 RCU / 25 WCU, TTL on `expires_at` | The free tier's whole allowance; no index. A table that exists with other keys is never touched |
| `iam` | Users `news-api-publish`, `news-mcp-read`, `news-site-read`, each with one inline policy | Policies are `iam/*.json` (`${REGION}`, `${ACCOUNT_ID}`, `${TABLE}` filled in). **No access keys are created**: Karthik makes them (below) |
| `edge` | Cache rule for article paths (`edge/cache-rules.json`), the one free-plan rate-limiting rule on `/mcp` (`edge/rate-limit-rules.json`), Worker routes (`edge/routes.json`) | Only rules whose `ref` starts with `news-` are managed; other rules in the zone are kept |

Worker routes (ADR 0010): `news.karthik.nz/*` to `news-site`, `/2*` to no Worker (so articles go to R2), `/mcp` and `/mcp/*` to `news-mcp`. The search API's hostname is not decided, so it has no route yet.

## Run it

```
bun run cloud:provision --dry-run   # the plan, nothing is called, no credentials needed
bun run cloud:provision --check     # read-only: what exists, what would be created or changed
bun run cloud:provision                # create or fix what is missing
bun run cloud:provision --only=d1,dynamodb   # some groups: r2, d1, dynamodb, iam, edge
```

The script is run with `--no-env-file`: it reads the shell's environment only, never `.env`. After a `--check` or a real run it prints each D1 database id, which goes into the `d1_databases` bindings of the Workers' `wrangler.jsonc` files (ids are not secrets).

## Environment

Set these in the shell for one run; none is stored in the repository. A missing or malformed value stops the run before any call (the message names the variable, never the value).

| Variable | Needed by | Meaning |
| --- | --- | --- |
| `CLOUDFLARE_API_TOKEN` | r2, d1, edge | A token for provisioning only (not the deploy token, not the purge token). See permissions below |
| `CLOUDFLARE_ACCOUNT_ID`, `CLOUDFLARE_ZONE_ID` | all | 32 hex characters each |
| `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN` (optional) | dynamodb, iam | An administrator or provisioning identity: it needs `dynamodb:CreateTable`, `DescribeTable`, `UpdateTable`, `DescribeTimeToLive`, `UpdateTimeToLive` and `iam:GetUser`, `CreateUser`, `GetUserPolicy`, `PutUserPolicy`. Not one of the three Worker users |
| `AWS_ACCOUNT_ID`, `AWS_REGION` | dynamodb, iam | 12 digits; for example `eu-west-1`. `NEWS_DYNAMODB_REGION` in the Workers must be the same |
| `NEWS_DOMAIN` | all | Default `news.karthik.nz` |
| `NEWS_R2_BUCKET` | r2 | Default `news-pages` |
| `NEWS_DYNAMODB_TABLE` | dynamodb, iam | Default `news` |
| `NEWS_SEARCH_YEARS` | d1 | Default `2026`. Add the next year in December (and its binding, then deploy: ADR 0010) |

Cloudflare token permissions (an approximation of what the calls need; if a call answers 403, add the permission it names): Account: Workers R2 Storage Edit, D1 Edit. Zone (`karthik.nz` only): Workers Routes Edit, Cache Settings Edit, Zone WAF Edit, DNS Edit (the R2 custom domain adds a DNS record). Create it with the shortest expiry that fits, and delete it after the first run.

## Deployment (CD)

`.github/workflows/deploy.yml` deploys the Workers and the site. It runs three ways:

- A push to `main`: `cd.yml` calls it when CI has completed successfully for that push (a `workflow_run` on the CI workflow, push events only); the plan stage deploys only if the commit is still the head of `main`.
- Run workflow in the Actions tab, or Board's Deploy button (`workflow_dispatch`).
- Only on `main`: a run on another branch is refused at the plan stage (a dry run, which deploys nothing, is the one exception), and the `production` environment admits `main` only.

A Worker or the site whose `wrangler.jsonc` still has a `TODO(` is not deployed (a notice on `all`, an error when picked by name). After the site deploys, a smoke test (`smoke.ts`) checks the live site: `robots.txt`, home, `/latest`, `/search`, `/sitemap.xml`, MCP `tools/list`, and the feed's first article from R2. It needs at least one published page; it tries each check three times, ten seconds apart.

Secrets, by name only, as secrets of the `production` environment (never repository secrets):

| Secret | Used by |
| --- | --- |
| `CLOUDFLARE_API_TOKEN` | `wrangler deploy`: Workers Scripts Edit on the account, plus the zone permissions for the routes if they are set in `wrangler.jsonc` |
| `CLOUDFLARE_ACCOUNT_ID` | `wrangler deploy` |

The Workers' own secrets (`NEWS_WRITE_API_KEY`, the DynamoDB keys, `CLOUDFLARE_PURGE_TOKEN`) are set once with `wrangler secret put` and are never in GitHub; see `workers/README.md`.

## What only Karthik can do

1. Create the Cloudflare and AWS accounts, the zone, the AWS budget alarm at $0.01, and the provisioning credentials above.
2. Run `--check`, then the real run, with those credentials.
3. In the AWS console, create one access key for each of the three users, and set them as Worker secrets with `wrangler secret put NEWS_DYNAMODB_ACCESS_KEY_ID` and `NEWS_DYNAMODB_SECRET_ACCESS_KEY` (publish key for `api`, read key for `mcp`, site key for `site`).
4. Put the printed D1 ids, the table name and region, the bucket name and the zone id into the three `wrangler.jsonc` files (the `TODO(` comments say where), and delete each `TODO(` line: that is what lets the deploy pick the Worker up.
5. Create the `production` environment with a required reviewer and `main` only, and its two secrets.
6. Check, after the first deploy, that a purge by URL removes a cached page (ADR 0010), and read one Workers log line for a client IP (invocation logs are off).

## Tests

`bun test scripts/tests/cloud-provision.test.ts scripts/tests/cloud-smoke.test.ts`: the plan against in-memory fakes of both APIs (dry run, a fresh account, a second run with no writes, drift) and the smoke test against a fake site. The real HTTP clients (`clients.ts`) are not exercised by any test.

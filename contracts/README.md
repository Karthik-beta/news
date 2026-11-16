# Contracts

The shapes that cross a boundary between parts of News: the laptop and the cloud, one Worker and another, the cloud and outside clients. JSON Schema 2020-12 and OpenAPI 3.1, as JSON. Code follows these files, not the other way round: change the contract first, in the same PR as the code.

| Folder | What | Owner (issue) | Used by |
| --- | --- | --- | --- |
| `bundle/` | A page bundle: `meta.schema.json` (meta.v1) and `manifest.schema.json` (manifest.v1) | Cloud data layer (#6); the renderer writes them | Renderer and `news publish` (laptop), write API |
| `cloud/` | Everything the cloud stores or serves | Cloud data layer (#6) | Workers, `news` CLI (#5), SSR site (#14) |

`cloud/` in detail:

| File | Contract |
| --- | --- |
| `common.schema.json` | IDs, paths, slugs, regions, `lang`, timestamps, enums shared by the rest |
| `dynamodb-items.schema.json` | Every DynamoDB item (ADR 0004) |
| `curation.v1.schema.json` | The home curation (ADR 0005) |
| `write-api.openapi.json`, `write-api.schema.json` | The write API: routes, bodies, error codes (ADR 0006) |
| `search-api.openapi.json` | The public search API (built in #14) |
| `report.schema.json` | Reader error reports: intake and stored record (in the REPORTS D1 database, not DynamoDB). Report text is untrusted input |
| `mcp-tools.schema.json` | MCP tool inputs and outputs (self-contained, sent as-is in tools/list) |

Rules:

- Every content record the cloud stores carries `lang` (pages, hub entries, entities, taxonomy nodes, redirects, path claims, curations, reports). Bookkeeping records do not: idempotency records and page locks.
- Identifiers never contain `#`, which separates DynamoDB key parts.
- Numbers are never in these contracts' text fields; figures live in `data.json` and are referenced by ID.
- After changing any `*.schema.json`, run `bun run validators` in `workers/shared` to regenerate the precompiled validators; a test fails until you do. `workers/shared/src/contracts.test.ts` checks that every `$ref` resolves.

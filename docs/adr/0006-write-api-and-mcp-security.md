# ADR 0006: Security of the write API, report intake and MCP server

- Status: accepted (Phase 6, issue #6)
- Date: 15 November 2026
- Contracts: `contracts/cloud/write-api.openapi.json`, `report.schema.json`, `mcp-tools.schema.json`

## Context

The write API changes everything readers see; the report intake and the MCP server take input from anyone. All three run on the Workers free plan (little CPU per request, a daily request quota) and must not run AI (TD principle 3).

## Decision

**One secret key.** Every write API route except `/health` and the report intake needs `Authorization: Bearer <NEWS_WRITE_API_KEY>`. The key lives only in the laptop's `.env` and as a Worker secret, is at least 32 characters, and is rotated by setting a new secret and a new `.env` value. Comparison hashes both sides with SHA-256 and compares the 32-byte digests with a constant-time loop, so neither the key's length nor the position of a mismatch leaks. With no key (or a short one) configured, every write is refused (fail closed). Authentication runs before any other check, including the method check, so an unauthenticated caller learns nothing about routes. Failed attempts are rate-limited per client IP (10 a minute). Keys, bodies, e-mails and IPs are never logged.

**Idempotency.** Every write needs an `Idempotency-Key` (Stripe-style; the publisher sends `<page_id>:<version>:<bundle_sha256>`). The API first reads `IDEM#<SHA-256 of key>`: a finished record with the same request hash (method, path, exact body) is replayed byte for byte with `Idempotent-Replayed: true` and nothing is written; a different hash is 409 `idempotency_key_reused`; a pending record younger than 2 minutes is 409 `idempotency_in_progress`. Otherwise the request is validated, a pending record is created with a create-only put, the operation runs, and the record becomes `done` with the response. A failure releases the record so the retry runs again. Records expire after 7 days (DynamoDB TTL on `expires_at`).

**Schema validation everywhere.** Every body is checked by validators compiled ahead of time from `contracts/` (`workers/shared/src/generated/validators.js`): workerd forbids runtime code generation and a free-plan request has little CPU. A unit test fails when the generated file is out of date. Schemas use closed objects (`additionalProperties: false`), anchored patterns without nested ambiguous quantifiers, and length limits. Error details carry JSON Pointers, never the offending values. Bodies are read with a hard cap enforced from `Content-Length` and again while streaming (pages 5 MiB, curation 64 KiB, small operations 4 KiB, reports 8 KiB, MCP 64 KiB). Bundle files are checked against their manifest hashes and sizes, the bundle hash is recomputed, file names come from a fixed list, and content types are set by the server.

**Least privilege.** The api Worker's DynamoDB key is publish-only (read and write on the one table, no table management). The MCP Worker gets a separate read-only key (`GetItem`, `BatchGetItem`, `Query`), only reads R2 and only runs `SELECT`s on D1. The purge token has only Zone > Cache Purge on the site's zone. The DynamoDB client refuses any endpoint except the region's official HTTPS endpoint or loopback (DynamoDB Local), so a bad setting cannot send signed requests elsewhere.

**Rate limits within the free plan.** Workers Rate Limiting bindings (`ratelimits` in `wrangler.jsonc`; free, no account setup): failed write-key attempts 10/min per IP; report intake 5/min per IP and 60/min overall; MCP 60/min per IP. Limits are per Cloudflare location and approximate, which is enough to stop floods; heavy MCP users are pointed at the bulk dumps.

**Report intake spam controls.** Size limit 8 KiB; JSON only; the per-IP and global limits; a hidden `website` honeypot (filled: answered 202 as if accepted, nothing stored); the page must exist and be published; requests from another site's Origin are refused (403) and CORS allows only the site. The stored record is the report text, kind, page, optional e-mail, language and receipt time; never the IP address, user agent or other request details. The IP is only the rate limiter's in-memory key. Records expire after 180 days; `news corrections pull` (Errata) reads them with the write key. The e-mail is stored in the cloud only so Karthik can reply; the Errata workflow keeps it out of the local store (Bureau Design 9.3).

**MCP server.** Stateless and read-only, MCP specification revision 2026-07-28 over Streamable HTTP at `POST /mcp`: no sessions (an `Mcp-Session-Id` is ignored and never minted), GET and DELETE are 405, each request carries its protocol version in `_meta` and the `MCP-Protocol-Version` header, `Mcp-Method` and `Mcp-Name` must match the body (400 `HeaderMismatch`). For older clients the server is dual-era: it answers `initialize` for 2025-11-25 and 2025-06-18 without creating a session. Browser Origins other than the site are refused (403, DNS-rebinding defence). Tools validate their arguments with the same precompiled validators; invalid arguments are tool errors the model can correct. Content returned by one call is capped (Markdown 60 KB, data 100 KB, whole result 512 KB) with truncation flagged and a link to the full file; figures are returned exactly as published.

**No AI in the cloud.** No Worker may import a model SDK or bind Workers AI. `workers/shared/src/ai-guard.test.ts` scans every Worker's sources, `package.json` and `wrangler.jsonc` and fails on any of them.

## Consequences

- A leaked write key allows writes until rotated; there is one key by design (TD Security). Rotation is a secret update plus a `.env` change.
- Rate limits are approximate; a determined distributed sender can exceed the global report limit by a little. The honeypot and the published-page check keep junk low.
- The MCP server's dual-era support can be dropped once clients have moved to 2026-07-28.

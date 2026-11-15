// news-api: the write API used by `news publish` and `news home publish` (TD "Cloud services"; ADRs 0004–0006).
// One secret key, schema-checked input, idempotent writes to R2, DynamoDB and D1, then cache purges.
// The handlers live in app.ts; this file only builds their dependencies from the Worker's bindings.
import { handle } from "./app";
import { depsFromEnv, type Env } from "./deps";

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    return handle(depsFromEnv(env), request);
  },
} satisfies ExportedHandler<Env>;

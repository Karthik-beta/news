// `bun run infra:up|down|logs`: Docker Compose for infra/docker-compose.yml with the .env values passed literally,
// so a `$` in a password is kept as typed, and an exported shell variable wins over .env (see composeEnv).
// Runs under `bun --no-env-file` so that Bun's `$`-expanded copy of .env never reaches process.env.
// Arguments are passed through to `docker compose`.
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { composeEnv, noEnvFileProblem } from "./env-file";
import { root } from "./lib";

const notReady = noEnvFileProblem(process.execArgv, "scripts/compose.ts");
if (notReady) {
  console.error(`✗ ${notReady}`);
  process.exit(1);
}

const envPath = join(root, ".env");
const text = existsSync(envPath) ? readFileSync(envPath, "utf8") : "";
const r = spawnSync("docker", ["compose", "-f", "infra/docker-compose.yml", ...process.argv.slice(2)], {
  cwd: root,
  stdio: "inherit",
  env: composeEnv(text, process.env),
});
if (r.error) console.error(`✗ could not run docker: ${r.error.message}`);
process.exit(r.status ?? 1);

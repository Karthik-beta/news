// `bun run infra:up|down|logs`: Docker Compose for infra/docker-compose.yml with the .env values passed literally,
// so a `$` in a password is kept as typed (see composeEnv). Arguments are passed through to `docker compose`.
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { composeEnv } from "./env-file";
import { root } from "./lib";

const envPath = join(root, ".env");
const text = existsSync(envPath) ? readFileSync(envPath, "utf8") : "";
const r = spawnSync("docker", ["compose", "-f", "infra/docker-compose.yml", ...process.argv.slice(2)], {
  cwd: root,
  stdio: "inherit",
  env: composeEnv(text, process.env),
});
if (r.error) console.error(`✗ could not run docker: ${r.error.message}`);
process.exit(r.status ?? 1);

// Runs the @visual screenshot tests inside the official Playwright Docker image, the only place baselines are
// made or compared (fonts render differently on Windows, macOS and Linux). Same image locally and in CI.
//   bun run test:visual                        compare against the committed baselines
//   bun run test:visual:update                 write new baselines (say why in the component's SIGNOFF.md)
//   bun run test:visual -- --project=smoke-*   any extra arguments go to `playwright test`
// The repo is copied onto the container's own disk and built there: building over a Windows bind mount took about a
// second per file, past the web server's 180 s timeout. The copy leaves out .git, node_modules, build output and the
// local secrets files (.env*, .dev.vars*), which the screenshot tests never need. Baselines and test-results/ come back.
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const repo = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const playwright = JSON.parse(readFileSync(join(repo, "node_modules/@playwright/test/package.json"), "utf8")).version;
const bun = process.versions.bun; // the container uses the same Bun as the host that runs this script
if (!bun) throw new Error("Run with Bun: bun run test:visual");
const image = `mcr.microsoft.com/playwright:v${playwright}-noble`;
const args = process.argv.slice(2).map((a) => `'${a.replace(/'/g, "'\\''")}'`);

const skip = [
  "./.git",
  "node_modules",
  "./site/dist",
  "./site/.wrangler",
  "./site/test-results",
  "./site/playwright-report",
  ".env",
  ".env.*",
  ".dev.vars",
  ".dev.vars.*",
];
const inside = [
  "set -e",
  `npm install --global --silent bun@${bun}`,
  "mkdir -p /build",
  `tar -C /work ${skip.map((s) => `--exclude='${s}'`).join(" ")} -cf - . | tar -xf - -C /build`,
  "cd /build && BUN_INSTALL_CACHE_DIR=/bun-cache bun install --frozen-lockfile",
  "cd site && NEWS_FIXTURES=1 NEWS_PREVIEW=1 CLOUDFLARE_ENV=local OPENUI_TELEMETRY_DISABLED=1 bun run build > /dev/null",
  "status=0",
  `E2E_SKIP_BUILD=1 npx playwright test --grep @visual ${args.join(" ")} || status=$?`,
  "mkdir -p /work/site/tests/e2e/__screenshots__",
  "cp -r tests/e2e/__screenshots__/. /work/site/tests/e2e/__screenshots__/ 2>/dev/null || true",
  "rm -rf /work/site/test-results && cp -r test-results /work/site/test-results 2>/dev/null || true",
  "exit $status",
].join("\n");

const result = spawnSync(
  "docker",
  [
    "run",
    "--rm",
    "--init",
    "--ipc=host",
    "-e",
    "CI=1",
    "-e",
    "NEWS_VISUAL=1",
    "-v",
    `${repo}:/work`,
    "-v",
    "news-visual-bun-cache:/bun-cache",
    "-w",
    "/work",
    image,
    "bash",
    "-lc",
    inside,
  ],
  { stdio: "inherit" },
);
process.exit(result.status ?? 1);

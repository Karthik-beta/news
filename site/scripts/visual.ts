// Runs the @visual screenshot tests inside the official Playwright Docker image, the only place baselines are
// made or compared (fonts render differently on Windows, macOS and Linux). Same image locally and in CI.
//   bun run test:visual                        compare against the committed baselines
//   bun run test:visual:update                 write new baselines (say why in the component's TESTING.md)
//   bun run test:visual -- --project=smoke-*   any extra arguments go to `playwright test`
import { spawnSync } from "node:child_process";
import { mkdtempSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const repo = join(dirname(fileURLToPath(import.meta.url)), "..", "..");

// The repo is mounted whole, so the local secrets files are covered with an empty read-only file: the screenshot
// tests need none of them. Only files that exist are covered; Docker would create a missing mount point on the host.
const empty = join(mkdtempSync(join(tmpdir(), "news-visual-")), "empty");
writeFileSync(empty, "");
const workers = readdirSync(join(repo, "workers"), { withFileTypes: true }).filter((d) => d.isDirectory());
const secretFiles = ["", "site", ...workers.map((w) => `workers/${w.name}`)].flatMap((dir) =>
  readdirSync(join(repo, dir))
    .filter((f) => /^(\.env|\.dev\.vars)(\..+)?$/.test(f) && !f.endsWith(".example"))
    .map((f) => (dir ? `${dir}/${f}` : f)),
);
const masks = secretFiles.flatMap((f) => ["-v", `${empty}:/work/${f}:ro`]);
const playwright = JSON.parse(readFileSync(join(repo, "node_modules/@playwright/test/package.json"), "utf8")).version;
const bun = process.versions.bun; // the container uses the same Bun as the host that runs this script
if (!bun) throw new Error("Run with Bun: bun run test:visual");
const image = `mcr.microsoft.com/playwright:v${playwright}-noble`;
const args = process.argv.slice(2).map((a) => `'${a.replace(/'/g, "'\\''")}'`);

// node_modules live in named volumes so the container never runs the host's Windows binaries.
const inside = [
  `npm install --global --silent bun@${bun}`,
  "bun install --frozen-lockfile",
  "cd site",
  `npx playwright test --grep @visual ${args.join(" ")}`,
].join(" && ");

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
    ...masks,
    "-v",
    "news-visual-node-modules:/work/node_modules",
    "-v",
    "news-visual-site-node-modules:/work/site/node_modules",
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

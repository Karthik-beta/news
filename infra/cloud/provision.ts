// Provisions News's cloud resources. Idempotent: run it twice and the second run changes nothing.
//   bun --no-env-file infra/cloud/provision.ts --dry-run   print the plan; no API is called, no credentials needed
//   bun --no-env-file infra/cloud/provision.ts --check     read-only: what exists and what would change
//   bun --no-env-file infra/cloud/provision.ts             create or fix whatever is missing
// Add `--only r2,d1,dynamodb,iam,edge` to run some groups. Settings and credentials come from the environment only
// (names in infra/cloud/README.md); this file holds none, and prints none.
import { type Aws, aws as awsClient, type Cf, cloudflare } from "./clients";
import { buildSteps, GROUPS, type Group, loadConfig, type Mode, run } from "./steps";

export interface Clients {
  cf: Cf;
  aws: Aws;
}
type Env = Record<string, string | undefined>;

function realClients(env: Env, groups: Group[], cfg: ReturnType<typeof loadConfig>): Clients {
  const need = (names: string[]) => {
    const missing = names.filter((n) => !env[n]);
    if (missing.length) throw new Error(`Not set: ${missing.join(", ")}`);
  };
  const cfGroups = groups.some((g) => ["r2", "d1", "edge"].includes(g));
  const awsGroups = groups.some((g) => ["dynamodb", "iam"].includes(g));
  if (cfGroups) need(["CLOUDFLARE_API_TOKEN"]);
  if (awsGroups) need(["AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY"]);
  const unused = () => new Proxy({}, { get: () => () => Promise.reject(new Error("Group not selected")) });
  return {
    cf: cfGroups ? cloudflare(env.CLOUDFLARE_API_TOKEN as string, cfg.accountId, cfg.zoneId) : (unused() as Cf),
    aws: awsGroups
      ? awsClient(
          {
            accessKeyId: env.AWS_ACCESS_KEY_ID as string,
            secretAccessKey: env.AWS_SECRET_ACCESS_KEY as string,
            sessionToken: env.AWS_SESSION_TOKEN || undefined,
          },
          cfg.region,
        )
      : (unused() as Aws),
  };
}

export async function main(
  argv: string[],
  env: Env,
  log: (line: string) => void,
  makeClients: (env: Env, groups: Group[], cfg: ReturnType<typeof loadConfig>) => Clients = realClients,
): Promise<number> {
  const mode: Mode = argv.includes("--dry-run") ? "dry-run" : argv.includes("--check") ? "check" : "apply";
  const only = argv
    .find((a) => a.startsWith("--only="))
    ?.slice(7)
    .split(",");
  const unknown = (only ?? []).filter((g) => !(GROUPS as readonly string[]).includes(g));
  if (unknown.length) {
    log(`Unknown group: ${unknown.join(", ")}. Groups: ${GROUPS.join(", ")}`);
    return 2;
  }
  const groups = (only as Group[] | undefined) ?? [...GROUPS];
  try {
    const cfg = loadConfig(env, mode === "dry-run");
    const clients = mode === "dry-run" ? undefined : makeClients(env, groups, cfg);
    // dry-run never gets a client: the steps are built over inert ones that fail if anything calls them.
    const none = new Proxy({}, { get: () => () => Promise.reject(new Error("dry-run must not call an API")) });
    const { steps, d1Ids } = buildSteps(cfg, (clients?.cf ?? none) as Cf, (clients?.aws ?? none) as Aws);
    const { changed } = await run(
      steps.filter((s) => groups.includes(s.group)),
      mode,
      log,
    );
    if (mode !== "dry-run" && d1Ids.size) {
      log("\nD1 ids for the wrangler.jsonc d1_databases bindings (not secrets):");
      for (const [name, id] of d1Ids) log(`  ${name}: ${id}`);
    }
    if (mode === "check") log(changed ? `\n${changed} step(s) would change.` : "\nNothing to change.");
    return 0;
  } catch (e) {
    log(`Failed: ${e instanceof Error ? e.message : String(e)}`);
    return 1;
  }
}

if (import.meta.main) process.exit(await main(process.argv.slice(2), process.env, console.log));

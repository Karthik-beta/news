// What provisioning creates, as a list of steps. Each step can say whether its resource is already right (check)
// and can make it right (apply); a step that finds nothing to do makes no write, which is what makes a second run a
// no-op. Pure logic over the Cf and Aws interfaces: no network and no process.env in this file.
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { type Aws, type Cf, MANAGED_TAG, type Route, type Rule } from "./clients";

const here = fileURLToPath(new URL(".", import.meta.url));
const read = (...p: string[]) => readFileSync(join(here, ...p), "utf8");

/** DynamoDB free tier: 25 read and 25 write capacity units in all. The table has no index, so this is the budget. */
export const RCU = 25;
export const WCU = 25;
export const TTL_ATTRIBUTE = "expires_at";
export const IAM_USERS = [
  { user: "news-api-publish", file: "news-api-publish.json" },
  { user: "news-mcp-read", file: "news-mcp-read.json" },
  { user: "news-site-read", file: "news-site-read.json" },
] as const;
export const GROUPS = ["r2", "d1", "dynamodb", "iam", "edge"] as const;
export type Group = (typeof GROUPS)[number];

export interface Config {
  accountId: string;
  zoneId: string;
  domain: string;
  bucket: string;
  years: number[];
  table: string;
  region: string;
  awsAccountId: string;
}

const ENV_RULES: { key: string; pattern: RegExp; hint: string; fallback?: string }[] = [
  { key: "CLOUDFLARE_ACCOUNT_ID", pattern: /^[0-9a-f]{32}$/, hint: "32 hex characters" },
  { key: "CLOUDFLARE_ZONE_ID", pattern: /^[0-9a-f]{32}$/, hint: "32 hex characters" },
  { key: "AWS_ACCOUNT_ID", pattern: /^\d{12}$/, hint: "12 digits" },
  { key: "AWS_REGION", pattern: /^[a-z]{2}(-[a-z]+)+-\d$/, hint: "for example eu-west-1" },
  {
    key: "NEWS_DOMAIN",
    pattern: /^[a-z0-9]([a-z0-9.-]*[a-z0-9])?\.[a-z]{2,}$/,
    hint: "a host name",
    fallback: "news.karthik.nz",
  },
  {
    key: "NEWS_R2_BUCKET",
    pattern: /^[a-z0-9][a-z0-9-]{1,61}[a-z0-9]$/,
    hint: "3-63 lowercase letters, digits, hyphens",
    fallback: "news-pages",
  },
  {
    key: "NEWS_DYNAMODB_TABLE",
    pattern: /^[A-Za-z0-9_.-]{3,255}$/,
    hint: "3-255 letters, digits, _ . -",
    fallback: "news",
  },
  { key: "NEWS_SEARCH_YEARS", pattern: /^20\d\d(,20\d\d)*$/, hint: "comma-separated years", fallback: "2026" },
];

/** Reads and validates the settings. Values reach URLs and policies, so each one is checked. Never echoes a value. */
export function loadConfig(env: Record<string, string | undefined>, placeholders: boolean): Config {
  const v: Record<string, string> = {};
  const problems: string[] = [];
  for (const r of ENV_RULES) {
    const raw = env[r.key]?.trim() || r.fallback;
    if (!raw) {
      if (placeholders) v[r.key] = `<${r.key}>`;
      else problems.push(`${r.key} is not set (${r.hint})`);
    } else if (!r.pattern.test(raw)) problems.push(`${r.key} is not valid (${r.hint})`);
    else v[r.key] = raw;
  }
  if (problems.length) throw new Error(`Settings problems:\n- ${problems.join("\n- ")}`);
  return {
    accountId: v.CLOUDFLARE_ACCOUNT_ID as string,
    zoneId: v.CLOUDFLARE_ZONE_ID as string,
    domain: v.NEWS_DOMAIN as string,
    bucket: v.NEWS_R2_BUCKET as string,
    years: (v.NEWS_SEARCH_YEARS as string).split(",").map(Number),
    table: v.NEWS_DYNAMODB_TABLE as string,
    region: v.AWS_REGION as string,
    awsAccountId: v.AWS_ACCOUNT_ID as string,
  };
}

export type State = "ok" | "missing" | "drift";
export interface Step {
  group: Group;
  /** What the step makes sure of, in one line (shown by --dry-run). */
  plan: string;
  check(): Promise<State>;
  apply(state: "missing" | "drift"): Promise<void>;
}

const fill = <T>(value: T, vars: Record<string, string>): T =>
  JSON.parse(JSON.stringify(value).replace(/\$\{(\w+)\}/g, (m, k: string) => vars[k] ?? m));

/** Is everything in `want` present in `have`? Fields the API adds on its side (defaults, ids) do not count. */
export function covers(have: unknown, want: unknown): boolean {
  if (Array.isArray(want))
    return Array.isArray(have) && have.length === want.length && want.every((w, i) => covers(have[i], w));
  if (want && typeof want === "object") {
    const h = have as Record<string, unknown> | null | undefined;
    return !!h && typeof h === "object" && Object.entries(want).every(([k, w]) => covers(h[k], w));
  }
  return have === want;
}

export function buildSteps(cfg: Config, cf: Cf, aws: Aws): { steps: Step[]; d1Ids: Map<string, string> } {
  const steps: Step[] = [];
  const d1Ids = new Map<string, string>();
  const vars = { DOMAIN: cfg.domain, REGION: cfg.region, ACCOUNT_ID: cfg.awsAccountId, TABLE: cfg.table };

  steps.push({
    group: "r2",
    plan: `R2 bucket ${cfg.bucket}`,
    check: async () => ((await cf.bucketExists(cfg.bucket)) ? "ok" : "missing"),
    apply: () => cf.createBucket(cfg.bucket),
  });
  steps.push({
    group: "r2",
    plan: `R2 custom domain ${cfg.domain} on ${cfg.bucket}`,
    // A bucket that does not exist yet has no domains, so a fresh state reads as missing without a call.
    check: async () =>
      (await cf.bucketExists(cfg.bucket)) && (await cf.customDomains(cfg.bucket)).includes(cfg.domain)
        ? "ok"
        : "missing",
    apply: () => cf.addCustomDomain(cfg.bucket, cfg.domain),
  });

  const migrations = (dir: string) =>
    readdirSync(join(here, "../..", dir))
      .filter((f) => f.endsWith(".sql"))
      .sort()
      .map((name) => ({ name, sql: readFileSync(join(here, "../..", dir, name), "utf8") }));
  const databases = [
    ...cfg.years.map((y) => ({ name: `news-search-${y}`, dir: "workers/shared/migrations/search" })),
    { name: "news-reports", dir: "workers/api/migrations/reports" },
  ];
  for (const db of databases) {
    const files = migrations(db.dir);
    // Wrangler's own tracking table, so `wrangler d1 migrations` agrees with what this script applied.
    const applied = async (id: string) => {
      const t = await cf.d1Query(id, "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'd1_migrations'");
      if (t.length === 0) return new Set<string>();
      return new Set((await cf.d1Query(id, "SELECT name FROM d1_migrations")).map((r) => String(r.name)));
    };
    steps.push({
      group: "d1",
      plan: `D1 database ${db.name}`,
      check: async () => {
        const id = await cf.findD1(db.name);
        if (id) d1Ids.set(db.name, id);
        return id ? "ok" : "missing";
      },
      apply: async () => void d1Ids.set(db.name, await cf.createD1(db.name)),
    });
    steps.push({
      group: "d1",
      plan: `D1 migrations on ${db.name}: ${files.map((f) => f.name).join(", ")}`,
      check: async () => {
        const id = d1Ids.get(db.name);
        if (!id) return "missing";
        const done = await applied(id);
        return files.every((f) => done.has(f.name)) ? "ok" : "missing";
      },
      apply: async () => {
        const id = d1Ids.get(db.name) as string;
        await cf.d1Query(
          id,
          "CREATE TABLE IF NOT EXISTS d1_migrations (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT UNIQUE, applied_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP)",
        );
        const done = await applied(id);
        for (const f of files.filter((f) => !done.has(f.name))) {
          // The file as it is (it has triggers, so it cannot be split on semicolons), then the record, in one call.
          await cf.d1Query(id, `${f.sql}\nINSERT INTO d1_migrations (name) VALUES ('${f.name}');`);
        }
      },
    });
  }

  steps.push({
    group: "dynamodb",
    plan: `DynamoDB table ${cfg.table} in ${cfg.region}: keys PK/SK, provisioned ${RCU} RCU / ${WCU} WCU, TTL on ${TTL_ATTRIBUTE}`,
    check: async () => {
      const t = await aws.describeTable(cfg.table);
      if (!t) return "missing";
      if (!t.managed)
        throw new Error(
          `Table ${cfg.table} exists without the tag ${MANAGED_TAG.Key}=${MANAGED_TAG.Value}, so this script did not make it: not changed. Pick another name, or tag it yourself if it is yours.`,
        );
      if (!t.keysOk)
        throw new Error(`Table ${cfg.table} exists with other keys than PK/SK: not changed. Pick another name.`);
      if (!t.provisioned || t.rcu !== RCU || t.wcu !== WCU) return "drift";
      return (await aws.ttlEnabled(cfg.table, TTL_ATTRIBUTE)) ? "ok" : "drift";
    },
    apply: async (state) => {
      const t = state === "missing" ? undefined : await aws.describeTable(cfg.table);
      if (!t) await aws.createTable(cfg.table, RCU, WCU);
      else if (!t.provisioned || t.rcu !== RCU || t.wcu !== WCU) await aws.setThroughput(cfg.table, RCU, WCU);
      if (!(await aws.ttlEnabled(cfg.table, TTL_ATTRIBUTE))) await aws.enableTtl(cfg.table, TTL_ATTRIBUTE);
    },
  });

  for (const { user, file } of IAM_USERS) {
    const doc = fill(JSON.parse(read("iam", file)), vars);
    steps.push({
      group: "iam",
      plan: `IAM user ${user} (no access key) with inline policy ${file}`,
      check: async () => {
        if (!(await aws.userExists(user))) return "missing";
        const have = await aws.getUserPolicy(user, user);
        // Managed policies and groups would add permissions this script does not know about.
        if ((await aws.userExtras(user)).length) return "drift";
        return covers(have, doc) && covers(doc, have) ? "ok" : "drift";
      },
      apply: async () => {
        const extras = (await aws.userExists(user)) ? await aws.userExtras(user) : [];
        // Never detaches or removes anything itself: that is a permissions decision for Karthik.
        if (extras.length)
          throw new Error(
            `IAM user ${user} has ${extras.join(", ")} besides its inline policy: remove them in IAM, then run again.`,
          );
        if (!(await aws.userExists(user))) await aws.createUser(user);
        await aws.putUserPolicy(user, user, doc);
      },
    });
  }

  for (const [phase, file, what] of [
    ["http_request_cache_settings", "cache-rules.json", "Cache rules"],
    ["http_ratelimit", "rate-limit-rules.json", "CDN rate-limiting rule"],
  ] as const) {
    const wanted = fill(JSON.parse(read("edge", file)) as Rule[], vars);
    // Only rules whose ref starts with "news-" are ours; other rules in the zone's phase are kept as they are.
    const mine = (r: Rule) => String(r.ref ?? "").startsWith("news-");
    steps.push({
      group: "edge",
      plan: `${what} (${phase}): ${wanted.map((r) => r.ref).join(", ")}`,
      check: async () => {
        const have = (await cf.getRules(phase)).filter(mine);
        if (have.length === 0) return "missing";
        return have.length === wanted.length &&
          wanted.every((w) =>
            covers(
              have.find((r) => r.ref === w.ref),
              w,
            ),
          )
          ? "ok"
          : "drift";
      },
      apply: async () =>
        void (await cf.putRules(phase, [...(await cf.getRules(phase)).filter((r) => !mine(r)), ...wanted])),
    });
  }

  const routes = fill(JSON.parse(read("edge", "routes.json")) as Route[], vars);
  const routeState = async () => {
    const have = new Map((await cf.routes()).map((r) => [r.pattern, r]));
    return routes.map((r) => ({ want: r, have: have.get(r.pattern) }));
  };
  steps.push({
    group: "edge",
    plan: `Worker routes: ${routes.map((r) => `${r.pattern} -> ${r.script ?? "no Worker (R2)"}`).join(", ")}`,
    check: async () => {
      const s = await routeState();
      if (s.some((x) => !x.have)) return "missing";
      return s.every((x) => x.have?.script === x.want.script) ? "ok" : "drift";
    },
    apply: async () => {
      for (const x of await routeState()) {
        if (!x.have || x.have.script !== x.want.script) await cf.setRoute({ ...x.want, id: x.have?.id });
      }
    },
  });

  return { steps, d1Ids };
}

/**
 * Stops a run whose credentials or settings point at another account: an account id in the settings is only a label
 * until the credentials are asked who they are. Called once, before any step, in check and apply (a dry run has no
 * clients). Only the providers of the selected groups are checked. Messages never carry a value.
 */
export async function verifyAccounts(cfg: Config, cf: Cf, aws: Aws, groups: Group[]): Promise<void> {
  if (groups.some((g) => g === "dynamodb" || g === "iam")) {
    if ((await aws.callerAccount()) !== cfg.awsAccountId)
      throw new Error("The AWS credentials belong to a different account than AWS_ACCOUNT_ID: nothing was changed.");
  }
  if (groups.some((g) => g === "r2" || g === "d1" || g === "edge")) {
    const zone = await cf.zone();
    if (zone.accountId !== cfg.accountId)
      throw new Error(
        "CLOUDFLARE_ZONE_ID belongs to a different account than CLOUDFLARE_ACCOUNT_ID: nothing was changed.",
      );
    if (!zone.name || (cfg.domain !== zone.name && !cfg.domain.endsWith(`.${zone.name}`)))
      throw new Error("NEWS_DOMAIN is not the zone CLOUDFLARE_ZONE_ID or a subdomain of it: nothing was changed.");
  }
}

export type Mode = "dry-run" | "check" | "apply";

/** Runs the steps. dry-run touches nothing (no client call at all); check only reads; apply writes what is not ok. */
export async function run(steps: Step[], mode: Mode, log: (line: string) => void): Promise<{ changed: number }> {
  let changed = 0;
  for (const step of steps) {
    if (mode === "dry-run") {
      log(`[plan]    ${step.plan}`);
      continue;
    }
    const state = await step.check();
    if (state === "ok") {
      log(`[ok]      ${step.plan}`);
      continue;
    }
    changed++;
    if (mode === "check") {
      log(`[${state === "missing" ? "create" : "update"}]  ${step.plan}`);
    } else {
      await step.apply(state);
      log(`[${state === "missing" ? "created" : "updated"}] ${step.plan}`);
    }
  }
  return { changed };
}

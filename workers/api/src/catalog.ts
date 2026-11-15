// Taxonomy nodes (PUT /v1/taxonomy) and redirects (PUT /v1/redirects). Categories are data, not code.
import { categoryHub, redirectKey, taxonomyNodeKey } from "@news/worker-shared/keys";
import { Reader } from "@news/worker-shared/read";
import type { RedirectRecord, TaxonomyNode } from "@news/worker-shared/records";
import { absolute, hubPath } from "@news/worker-shared/urls";
import { check, type Issue, schemas } from "@news/worker-shared/validate";
import type { Deps } from "./deps";
import { ApiError } from "./errors";
import type { OpResult } from "./pages";

type NodeInput = Pick<TaxonomyNode, "id" | "name" | "parent" | "order" | "lang">;
type RedirectInput = Pick<RedirectRecord, "from" | "to" | "status">;

const parentOf = (slug: string) => (slug.includes("/") ? slug.slice(0, slug.lastIndexOf("/")) : null);

export async function prepareTaxonomy(deps: Deps, body: unknown): Promise<{ nodes: NodeInput[] }> {
  const r = check<{ nodes: NodeInput[] }>(schemas.validateTaxonomyRequest, body);
  if (!r.ok) throw new ApiError("invalid_request", "the request body does not match taxonomyRequest", r.issues);
  const issues: Issue[] = [];
  const ids = new Set<string>();
  const existing = new Set((await new Reader(deps.table()).listTaxonomy()).map((n) => n.id));
  r.value.nodes.forEach((n, i) => {
    if (ids.has(n.id))
      issues.push({ code: "taxonomy.duplicate", message: `${n.id} is listed twice`, path: `/nodes/${i}` });
    ids.add(n.id);
    // The tree follows the slugs: 'economy/inflation' sits under 'economy', a top-level slug has no parent.
    if (n.parent !== parentOf(n.id))
      issues.push({
        code: "taxonomy.parent",
        message: `${n.id} must have parent ${parentOf(n.id)}`,
        path: `/nodes/${i}/parent`,
      });
  });
  r.value.nodes.forEach((n, i) => {
    if (n.parent && !ids.has(n.parent) && !existing.has(n.parent))
      issues.push({
        code: "taxonomy.parent_missing",
        message: `parent ${n.parent} does not exist`,
        path: `/nodes/${i}/parent`,
      });
  });
  if (issues.length > 0) throw new ApiError("invalid_request", "the taxonomy is not a consistent tree", issues);
  return r.value;
}

export async function executeTaxonomy(deps: Deps, p: { nodes: NodeInput[] }): Promise<OpResult> {
  const table = deps.table();
  const now = deps.now().toISOString();
  for (const n of p.nodes) {
    const item: TaxonomyNode = {
      ...taxonomyNodeKey(n.id),
      PK: "TAX#tree",
      type: "taxonomy_node",
      ...n,
      updated_at: now,
    };
    await table.put(item as never);
  }
  const purged = await deps.purger.purge(p.nodes.map((n) => absolute(deps.siteOrigin, hubPath(categoryHub(n.id)))));
  return { status: 200, body: { written: p.nodes.length, purged } };
}

export async function prepareRedirects(_deps: Deps, body: unknown): Promise<{ redirects: RedirectInput[] }> {
  const r = check<{ redirects: RedirectInput[] }>(schemas.validateRedirectsRequest, body);
  if (!r.ok) throw new ApiError("invalid_request", "the request body does not match redirectsRequest", r.issues);
  const issues: Issue[] = [];
  const from = new Set<string>();
  r.value.redirects.forEach((x, i) => {
    if (x.from === x.to)
      issues.push({ code: "redirect.loop", message: "a redirect cannot point at itself", path: `/redirects/${i}` });
    if (from.has(x.from))
      issues.push({ code: "redirect.duplicate", message: `${x.from} is listed twice`, path: `/redirects/${i}` });
    from.add(x.from);
  });
  if (r.value.redirects.some((x) => from.has(x.to)))
    issues.push({ code: "redirect.chain", message: "a redirect target is itself redirected in this request" });
  if (issues.length > 0) throw new ApiError("invalid_request", "the redirects are not consistent", issues);
  return r.value;
}

export async function executeRedirects(deps: Deps, p: { redirects: RedirectInput[] }): Promise<OpResult> {
  const table = deps.table();
  const now = deps.now().toISOString();
  for (const x of p.redirects) {
    const item: RedirectRecord = {
      ...redirectKey(x.from),
      SK: "REDIRECT",
      type: "redirect",
      lang: "en",
      ...x,
      created_at: now,
    };
    await table.put(item as never);
  }
  const purged = await deps.purger.purge(p.redirects.map((x) => absolute(deps.siteOrigin, x.from)));
  return { status: 200, body: { written: p.redirects.length, purged } };
}

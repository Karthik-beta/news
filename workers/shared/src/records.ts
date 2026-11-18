// TypeScript shapes of the items in contracts/cloud/dynamodb-items.schema.json and the bundle meta.
// The JSON Schemas are the source of truth; these mirror them for the compiler.

export type PageType = "earnings" | "macro" | "policy" | "report" | "news" | "science" | "explainer";
export type IndexStatus = "index" | "noindex";
export type EntityKind = "company" | "person" | "country" | "institution" | "topic" | "source";

export interface EntityRef {
  id: string;
  name: string;
  kind: EntityKind;
}

/** contracts/bundle/meta.schema.json */
export interface PageMeta {
  schema: "meta.v1";
  id: string;
  path: string;
  version: number;
  lang: string;
  title: string;
  fly_title?: string;
  summary: string;
  page_type: PageType;
  tier: number;
  categories: string[];
  entities: EntityRef[];
  tags: string[];
  region: string;
  published_at: string;
  updated_at: string;
  index_status: IndexStatus;
  renderer_version: string;
  data_ids: string[];
  chart_ids: string[];
  flags?: string[];
}

/** contracts/bundle/manifest.schema.json */
export interface Manifest {
  schema: "manifest.v1";
  page_id: string;
  version: number;
  bundle_sha256: string;
  files: { path: string; sha256: string; bytes: number }[];
}

export interface PageRecord {
  PK: string;
  SK: string;
  type: "page";
  lang: string;
  page_id: string;
  path: string;
  version: number;
  status: "published" | "withdrawn";
  index_status: IndexStatus;
  title: string;
  fly_title?: string;
  summary: string;
  page_type: PageType;
  tier: number;
  categories: string[];
  entities: EntityRef[];
  tags: string[];
  region: string;
  published_at: string;
  updated_at: string;
  renderer_version: string;
  data_ids: string[];
  chart_ids: string[];
  flags?: string[];
  bundle_sha256: string;
  files: string[];
  hubs: string[];
  written_at: string;
  withdrawn_at?: string;
  withdraw_reason?: string;
  /** LATEST only: hubs and files the replaced version had and this one dropped, until they are removed and purged. */
  cleanup?: { hubs: string[]; files: string[] };
}

export interface HubEntry {
  PK: string;
  SK: string;
  type: "hub_entry";
  lang: string;
  hub: string;
  page_id: string;
  path: string;
  title: string;
  fly_title?: string;
  summary: string;
  page_type: PageType;
  tier: number;
  region: string;
  published_at: string;
  updated_at: string;
  index_status: IndexStatus;
}

export interface EntityRecord {
  PK: string;
  SK: "META";
  type: "entity";
  lang: string;
  id: string;
  name: string;
  kind: EntityKind;
  updated_at: string;
}

export interface TaxonomyNode {
  PK: "TAX#tree";
  SK: string;
  type: "taxonomy_node";
  lang: string;
  id: string;
  name: string;
  parent: string | null;
  order: number;
  updated_at: string;
}

export interface RedirectRecord {
  PK: string;
  SK: "REDIRECT";
  type: "redirect";
  lang: string;
  from: string;
  to: string;
  status: 301 | 308;
  created_at: string;
}

export interface PathClaim {
  PK: string;
  SK: "PAGE";
  type: "path_claim";
  lang: string;
  page_id: string;
  path: string;
  created_at: string;
}

export interface PageLock {
  PK: string;
  SK: "LOCK";
  type: "page_lock";
  owner: string;
  acquired_at: string;
  expires_at: number;
}

export interface CurationSlot {
  page_id: string;
  figure_id?: string;
  chart_id?: string;
}

export interface LeadSlot extends CurationSlot {
  still_leading?: boolean;
}

/** contracts/cloud/curation.v1.schema.json */
export interface Curation {
  schema: "curation.v1";
  lang: string;
  created_at: string;
  approved_at: string;
  curator_model: string;
  slots: {
    lead: LeadSlot;
    side: CurationSlot[];
    sections: { category: string; pages: CurationSlot[] }[];
  };
  checks: { passed: true; results: { rule: string; ok: boolean; detail?: string }[] };
}

export interface CurationRecord extends Curation {
  PK: "HOME#CURATION";
  SK: string;
  type: "curation";
  version: number;
  published_at: string;
  made_live_at?: string;
  /** SHA-256 of the Idempotency-Key that published this version. */
  request_id?: string;
  server_checks: { passed: true; run_at: string };
}

export interface IdempotencyRecord {
  PK: string;
  SK: "META";
  type: "idempotency";
  request_hash: string;
  state: "pending" | "done";
  operation: string;
  status_code?: number;
  response_body?: string;
  created_at: string;
  expires_at: number;
}

/** IDEM#<key hash> / RESULT: what a committed operation produced, written with its commit (ADR 0005). */
export interface OperationResult {
  PK: string;
  SK: "RESULT";
  type: "operation_result";
  operation: "home_curation" | "home_rollback" | "withdraw";
  request_hash: string;
  /** home_curation: the version published; home_rollback: the newest version; withdraw: the page version withdrawn. */
  version: number;
  /** home_rollback: the version made live. */
  live_version?: number;
  /** withdraw: the page and its index status when it was withdrawn. */
  page_id?: string;
  index_status?: IndexStatus;
  created_at: string;
  expires_at: number;
}

export type ReportKind = "figure" | "wording" | "link" | "image" | "other";

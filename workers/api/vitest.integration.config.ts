import { fileURLToPath } from "node:url";
import { integrationConfig } from "@news/worker-test-support/vitest";

export default integrationConfig({
  root: import.meta.dirname,
  d1: ["SEARCH_DB", "REPORTS"],
  r2: ["PAGES"],
  dynamodb: true,
  searchShards: ["SEARCH_2025", "SEARCH_2026"],
  migrations: { REPORTS_MIGRATIONS: fileURLToPath(new URL("./migrations/reports", import.meta.url)) },
});

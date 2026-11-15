import { integrationConfig } from "@news/worker-test-support/vitest";

export default integrationConfig({
  root: import.meta.dirname,
  d1: ["SEARCH_DB"],
  r2: ["PAGES"],
  dynamodb: true,
  searchShards: ["SEARCH_2025", "SEARCH_2026"],
});

import { integrationConfig } from "@news/worker-test-support/vitest";

export default integrationConfig({
  root: import.meta.dirname,
  r2: ["PAGES"],
  dynamodb: true,
  searchShards: ["SEARCH_2026"],
});

import { integrationConfig } from "@news/worker-test-support/vitest";

export default integrationConfig({ root: import.meta.dirname, d1: ["SEARCH_DB"] });

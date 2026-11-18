// Unit tests for the shared code: plain Node, no Worker runtime needed (the code under test only uses web standards).
import { defineConfig } from "vitest/config";

export default defineConfig({
  root: import.meta.dirname,
  test: { include: ["src/**/*.test.ts"], environment: "node" },
});

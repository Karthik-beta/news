import { expect, test } from "bun:test";
import { PIPELINE_PHASE } from "./index";

test("pipeline placeholder points at Phase 7", () => {
  expect(PIPELINE_PHASE).toBe(7);
});

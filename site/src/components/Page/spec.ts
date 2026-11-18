import type { DefinedComponent } from "@openuidev/react-lang";
import { z } from "zod/v4";
import { defineRootComponent } from "../contract";
import { Page } from "./Page";

export default defineRootComponent({
  kind: "component",
  root: true,
  name: "Page",
  index: "Page — root; blocks in reading order — children[]",
  description: "The page: every block between the head and the sources, in reading order.",
  dataShape: "No data of its own; children are other components.",
  example: `root = Page([intro, chart])`,
  props: (children: DefinedComponent[]) => {
    const refs = children.map((c) => c.ref);
    const child =
      refs.length === 0
        ? z.never()
        : refs.length === 1
          ? (refs[0] as z.ZodType)
          : z.union(refs as [z.ZodType, z.ZodType]);
    return z.object({ children: z.array(child) });
  },
  component: Page,
  markdown: ({ props, children }) => children(props.children),
});

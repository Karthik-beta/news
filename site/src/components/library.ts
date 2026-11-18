// Builds the OpenUI library (createLibrary) and component-index.json from component specs. The list of specs is
// generated from the folders (registry.generated.ts); nothing here is listed by hand.
import { createLibrary, type DefinedComponent, defineComponent, type Library } from "@openuidev/react-lang";
import type { z } from "zod/v4";
import { type AnyComponentSpec, type AnySpec, estimateTokens, INDEX_BUDGET, type RootSpec } from "./contract";
import { specs as generatedSpecs } from "./registry.generated";

export interface NewsLibrary {
  library: Library;
  /** Component specs by name, for the renderer's Markdown and CSV hooks. */
  specs: Map<string, AnyComponentSpec>;
}

export const componentSpecs = (all: readonly AnySpec[]): AnyComponentSpec[] =>
  all.filter((s): s is AnyComponentSpec => s.kind === "component");

/**
 * An OpenUI library from component specs: exactly one root (Page), whose children may be any other component.
 * The renderer's tests and fixture pages pass extra specs; production uses the generated list.
 */
export function createNewsLibrary(specs: readonly AnyComponentSpec[]): NewsLibrary {
  const roots = specs.filter((s): s is RootSpec => s.root === true);
  if (roots.length !== 1) throw new Error(`A library needs exactly one root component, found ${roots.length}`);
  const root = roots[0] as RootSpec;
  const seen = new Set<string>();
  for (const s of specs) {
    if (seen.has(s.name)) throw new Error(`Two components are named ${s.name}`);
    seen.add(s.name);
  }
  const children: DefinedComponent[] = specs
    .filter((s) => s.root !== true)
    .map((s) =>
      defineComponent({
        name: s.name,
        description: s.description,
        props: s.props as z.ZodObject,
        component: s.component as never,
      }),
    );
  const rootComponent = defineComponent({
    name: root.name,
    description: root.description,
    props: root.props(children),
    component: root.component as never,
  });
  return {
    library: createLibrary({ components: [rootComponent, ...children], root: root.name }),
    specs: new Map(specs.map((s) => [s.name, s])),
  };
}

let production: NewsLibrary | undefined;

/** The library page writers use: every component folder with `kind: "component"`. */
export function newsLibrary(): NewsLibrary {
  production ??= createNewsLibrary(componentSpecs(generatedSpecs));
  return production;
}

export interface ComponentIndex {
  version: 1;
  /** ADR 0001: the whole index fits 4,000 tokens; no entry is over 24. Counted as ceil(UTF-8 bytes / 3). */
  budget: { tokens: number; perEntry: number };
  tokens: number;
  components: { name: string; line: string; tokens: number }[];
}

/** component-index.json: one line per component, in name order, for `news components` and the cached prefix. */
export function buildComponentIndex(specs: readonly AnyComponentSpec[]): ComponentIndex {
  const components = [...specs]
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((s) => ({ name: s.name, line: s.index, tokens: estimateTokens(s.index) }));
  return {
    version: 1,
    budget: { tokens: INDEX_BUDGET.total, perEntry: INDEX_BUDGET.perEntry },
    tokens: estimateTokens(components.map((c) => c.line).join("\n")),
    components,
  };
}

/** Problems with an index against the ADR 0001 budget; empty when it fits. */
export function indexBudgetProblems(index: ComponentIndex): string[] {
  const problems = index.components
    .filter((c) => c.tokens > index.budget.perEntry)
    .map((c) => `${c.name}: ${c.tokens} tokens, over the ${index.budget.perEntry}-token entry budget`);
  if (index.tokens > index.budget.tokens) {
    problems.push(`the index is ${index.tokens} tokens, over the ${index.budget.tokens}-token budget`);
  }
  return problems;
}

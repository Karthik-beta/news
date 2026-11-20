// Builds the OpenUI library (createLibrary) and component-index.json from component specs. The list of specs is
// generated from the folders (registry.generated.ts); nothing here is listed by hand.
import { createLibrary, type DefinedComponent, defineComponent, type Library } from "@openuidev/react-lang";
import { z } from "zod/v4";
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
  const library = createLibrary({ components: [rootComponent, ...children], root: root.name });
  // OpenUI's Renderer.Root asks for the library's JSON Schema on every render (a useMemo, which does not survive
  // between server renders), and building it walks every component's zod schema: about 2 ms for the fixture
  // library and growing with each component. The schema never changes for a library, so build it once.
  const schema = library.toJSONSchema();
  library.toJSONSchema = () => schema;
  return {
    library,
    specs: new Map(specs.map((s) => [s.name, s])),
  };
}

let production: NewsLibrary | undefined;

/** The library page writers use: every component folder with `kind: "component"`. */
export function newsLibrary(): NewsLibrary {
  production ??= createNewsLibrary(componentSpecs(generatedSpecs));
  return production;
}

export interface ComponentIndexEntry {
  name: string;
  /** The one line for the cached prompt prefix; the only field the token budget counts. */
  line: string;
  tokens: number;
  /** Details for `news component <name>` (contracts/component-index.schema.json); never in the prefix. */
  description: string;
  dataShape: string;
  example: string;
  /** JSON Schema 2020-12 of the props, from the zod schema. Left out for the root, whose children are the library. */
  props?: Record<string, unknown>;
}

export interface ComponentIndex {
  version: 1;
  /** ADR 0001: the whole index fits 4,000 tokens; no entry is over 24. Counted as ceil(UTF-8 bytes / 3). */
  budget: { tokens: number; perEntry: number };
  /** Tokens of the lines alone, joined by newlines: what `news components` puts in the cached prefix. */
  tokens: number;
  components: ComponentIndexEntry[];
}

/** The props' JSON Schema without the top-level `$schema` key (the index file states the dialect once, in the contract). */
function propsSchema(spec: AnyComponentSpec): Record<string, unknown> | undefined {
  if (spec.root === true) return undefined;
  const { $schema: _dialect, ...schema } = z.toJSONSchema(spec.props as z.ZodObject) as Record<string, unknown>;
  return schema;
}

/**
 * component-index.json: one entry per component, in name order. `line` is what `news components` puts in the
 * cached prefix (and what the budget counts); description, dataShape, example and props are the details
 * `news component <name>` returns on demand.
 */
export function buildComponentIndex(specs: readonly AnyComponentSpec[]): ComponentIndex {
  const components = [...specs]
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((s) => {
      const props = propsSchema(s);
      return {
        name: s.name,
        line: s.index,
        tokens: estimateTokens(s.index),
        description: s.description,
        dataShape: s.dataShape,
        example: s.example,
        ...(props ? { props } : {}),
      };
    });
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

// The component folder contract (site/src/components/README.md). Every folder's spec.ts default-exports one of
// these. The OpenUI library (createLibrary) and component-index.json are generated from them; nothing is listed
// by hand. Primitives are building blocks for components and never appear in the page writer's library or index.
import type { ComponentRenderer, DefinedComponent } from "@openuidev/react-lang";
import type { ReactNode } from "react";
import type { z } from "zod/v4";
import type { PageDataResolver } from "../renderer/data/resolve";

/** Arguments to the Markdown and CSV hooks: the same props the component got, plus the page's data. */
export interface HookArgs<P> {
  props: P;
  /** The statement name in page.oui ("chart"), when the component is a named statement. */
  statementId: string | undefined;
  data: PageDataResolver;
  /** Markdown for child nodes (any value the component's props hold that contains elements). */
  children: (value: unknown) => string;
}

/** A chart's or table's data for the CSV and JSON downloads: exact source values, never formatted. */
export interface ChartTable {
  title: string;
  subtitle?: string;
  /** Machine-readable column keys and their human labels. The first column is usually the period. */
  columns: { key: string; label: string }[];
  /** One object per row; values are exact decimal strings, verbatim labels, or null for missing. */
  rows: Record<string, string | null>[];
  unit?: string;
  currency?: string | null;
  scale?: string | null;
  /** Source IDs the rows come from. A source with redistribution=false blocks the download (check C8). */
  sources: string[];
}

interface SpecBase<P extends z.ZodObject> {
  kind: "component";
  /** PascalCase, the name the page writer calls in OpenUI Lang. Must match the folder name. */
  name: string;
  /**
   * The one-line entry for `news components` and component-index.json: "Name — purpose — data shape". At most 24
   * tokens (ADR 0001); the build test enforces it.
   */
  index: string;
  /** One sentence for OpenUI's prompt and `news component <type>`. */
  description: string;
  /** The data shape in words: which references each prop takes ("sr_… series of one measure"). */
  dataShape: string;
  /** One realistic example in OpenUI Lang, using references only. */
  example: string;
  component: ComponentRenderer<z.infer<P>>;
  /** Markdown for index.md. Without it, the component's children are written out in order. */
  markdown?: (args: HookArgs<z.infer<P>>) => string;
  /** The CSV/JSON download for a chart or table. Null when there is nothing to download. */
  csv?: (args: HookArgs<z.infer<P>>) => ChartTable | null;
}

export interface ComponentSpec<P extends z.ZodObject = z.ZodObject> extends SpecBase<P> {
  root?: false;
  props: P;
}

/**
 * The page root (Page). Its children prop lists every other component in the library, so its schema is built
 * when the library is.
 */
export interface RootSpec<P extends z.ZodObject = z.ZodObject> extends SpecBase<P> {
  root: true;
  props: (children: DefinedComponent[]) => P;
}

export type AnyComponentSpec = ComponentSpec | RootSpec;

/** A building block used inside components: documented and signed off like a component, but not in the library. */
export interface PrimitiveSpec {
  kind: "primitive";
  name: string;
  /** What it is for, in one sentence. */
  purpose: string;
  /** A short usage example in TSX. */
  example: string;
}

export interface FixtureCase {
  name: string;
  render: () => ReactNode;
}

export const defineNewsComponent = <P extends z.ZodObject>(spec: ComponentSpec<P>): ComponentSpec<P> => spec;
export const defineRootComponent = <P extends z.ZodObject>(spec: RootSpec<P>): RootSpec<P> => spec;
export const definePrimitive = (spec: Omit<PrimitiveSpec, "kind">): PrimitiveSpec => ({ kind: "primitive", ...spec });

export type AnySpec = AnyComponentSpec | PrimitiveSpec;

/** ceil(UTF-8 bytes / 3): the ADR 0001 estimate. It over-counts for Claude, so passing it leaves headroom. */
export const estimateTokens = (text: string): number => Math.ceil(new TextEncoder().encode(text).length / 3);

export const INDEX_BUDGET = { total: 4000, perEntry: 24 } as const;

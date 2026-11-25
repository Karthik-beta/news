// Sankey geometry (design/charts.md, "Sankey"): revenue segments flow into total revenue, which splits into costs
// and the profit that carries on to the next total, down to the result. The diagram is built from the same rows as
// the waterfall (totals, deductions, additions), so the two can never tell different stories. Pure and
// server-side: the island gets the finished geometry as plain JSON.
//
// Units: one unit is one CSS pixel at the default 16px root size (the plot's height is set in rem from it), so the
// 2px minimum flow is 2px on screen and grows with the reader's text size. x runs 0-1000 across the flows' area.
import type { ColourRole } from "../ChartFigure/model";
import { addExact, compareExact, effectOf, negateExact, type StepKind, sumExact } from "../Waterfall/steps";

export interface SankeyLineInput {
  label: string;
  ref: string;
  /** A revenue segment, or a statement row (as in the waterfall). */
  kind: "segment" | StepKind;
  /** The value as published, an exact decimal; null when the source has none. */
  value: string | null;
  /** The label's value as readers see it ("$201.2bn"). */
  shown: string;
}

/** One line behind a node: a folded "Other" node has several. */
export interface SankeyItem {
  label: string;
  ref: string;
  value: string;
  shown: string;
}

export interface SankeyNode {
  key: string;
  label: string;
  column: number;
  role: ColourRole;
  /** The lines behind the node (one, or every line folded into "Other"). */
  items: SankeyItem[];
  folded: boolean;
  /** True for the result (the last total). */
  result: boolean;
  /** Top and height in units. */
  y: number;
  height: number;
  /** Top of the node's label, in units, after nudging labels apart. */
  labelTop: number;
}

export interface SankeyLink {
  key: string;
  source: number;
  target: number;
  role: ColourRole;
  /** The amount the flow carries (geometry only; never shown). */
  value: number;
  /** Drawn thickness in units: in proportion to the value, never under the minimum. */
  thickness: number;
  /** Top of the band where it leaves its source and where it reaches its target, in units. */
  y0: number;
  y1: number;
  /** The node whose tooltip a pointer on this flow shows: the segment, the cost, the addition or the next total. */
  owner: number;
}

export interface SankeyLayout {
  columns: number;
  height: number;
  nodes: SankeyNode[];
  links: SankeyLink[];
}

export type SankeyResult =
  | { ok: true; layout: SankeyLayout }
  | {
      ok: false;
      reason: "missing" | "first-not-total" | "not-positive" | "negative-segment" | "mismatch" | "trailing-addition";
    };

export const SANKEY = {
  /** The plot's starting height, in units (26rem). */
  height: 416,
  /** No flow is drawn thinner than this, in units (design/charts.md). */
  minThickness: 2,
  /** Gap between nodes in a column, in units. */
  pad: 16,
  /** Flows under this share of total revenue fold into "Other" (design/charts.md). */
  foldShare: 0.01,
  /** A label's line height in units: the label size (13.5px) × 1.35. */
  lineHeight: 18.25,
  /** Characters a label's line holds before it wraps (7em of Fira Sans at the label size, at 0.55em a character). */
  lineChars: 12,
  /** Space between stacked labels, in units. */
  labelGap: 4,
} as const;

/**
 * Lines a name wraps to at `chars` a line: greedy, by words, a name also breaking after a slash; a word longer than
 * a line breaks across as many as it needs.
 */
export function wrapLines(text: string, chars: number = SANKEY.lineChars): number {
  let lines = 0;
  let current = 0;
  for (const word of text.split(/\s+|(?<=\/)/).filter(Boolean)) {
    if (current > 0 && current + 1 + word.length <= chars) {
      current += 1 + word.length;
      continue;
    }
    // A new line; a word longer than a line breaks across as many as it needs.
    lines += Math.ceil(word.length / chars);
    current = word.length % chars || chars;
  }
  return Math.max(lines, 1);
}

/** A label's height in units: its name, wrapped, and the value on a line of its own (none for "Other"). */
export function labelHeight(node: Pick<SankeyNode, "label" | "folded">): number {
  return (wrapLines(node.label) + (node.folded ? 0 : 1)) * SANKEY.lineHeight + SANKEY.labelGap;
}

interface DraftNode {
  label: string;
  column: number;
  role: ColourRole;
  items: SankeyItem[];
  folded: boolean;
  result: boolean;
  /** Where the node sits in its column: inflows on top, then the total, then the costs. */
  rank: number;
}

interface DraftLink {
  source: number;
  target: number;
  role: ColourRole;
  value: string;
  owner: number;
}

const item = (l: SankeyLineInput): SankeyItem => ({
  label: l.label,
  ref: l.ref,
  value: l.value as string,
  shown: l.shown,
});

/**
 * Builds the graph from segments and rows, or says why it cannot be drawn (then the chart shows the waterfall at
 * every width). A Sankey needs every value, a first row that is a total, totals above zero, segments not below
 * zero, totals that the lines above them add up to exactly, and no addition after the last total.
 */
export function sankeyGraph(
  lines: readonly SankeyLineInput[],
): { ok: true; nodes: DraftNode[]; links: DraftLink[]; revenue: string } | Extract<SankeyResult, { ok: false }> {
  if (lines.some((l) => l.value === null || !Number.isFinite(Number(l.value)))) return { ok: false, reason: "missing" };
  const segments = lines.filter((l) => l.kind === "segment");
  const rows = lines.filter((l) => l.kind !== "segment");
  const first = rows[0];
  if (first?.kind !== "total") return { ok: false, reason: "first-not-total" };
  if (rows.some((r) => r.kind === "total" && compareExact(r.value as string, "0") <= 0)) {
    return { ok: false, reason: "not-positive" };
  }
  if (segments.some((s) => compareExact(s.value as string, "0") < 0)) return { ok: false, reason: "negative-segment" };
  let lastTotal = -1;
  rows.forEach((r, i) => {
    if (r.kind === "total") lastTotal = i;
  });

  const base = segments.length > 0 ? 1 : 0;
  const nodes: DraftNode[] = [];
  const links: DraftLink[] = [];
  const addNode = (n: Omit<DraftNode, "folded">) => nodes.push({ ...n, folded: false }) - 1;

  let total = addNode({
    label: first.label,
    column: base,
    role: lastTotal === 0 ? "highlight" : "comparison",
    items: [item(first)],
    result: lastTotal === 0,
    rank: 1,
  });
  for (const s of segments) {
    if (compareExact(s.value as string, "0") === 0) continue;
    const n = addNode({ label: s.label, column: 0, role: "context", items: [item(s)], result: false, rank: 1 });
    links.push({ source: n, target: total, role: "context", value: s.value as string, owner: n });
  }
  if (segments.length > 0 && compareExact(sumExact(segments.map((s) => s.value as string)), first.value as string)) {
    return { ok: false, reason: "mismatch" };
  }

  let running = first.value as string;
  let outflow = "0";
  let pendingIn: { node: number; value: string }[] = [];
  for (let i = 1; i < rows.length; i++) {
    const r = rows[i] as SankeyLineInput;
    const column = (nodes[total] as DraftNode).column;
    if (r.kind === "total") {
      const carried = addExact(running, negateExact(outflow));
      const expected = addExact(carried, sumExact(pendingIn.map((p) => p.value)));
      if (compareExact(expected, r.value as string) !== 0) return { ok: false, reason: "mismatch" };
      const isResult = i === lastTotal;
      const next = addNode({
        label: r.label,
        column: column + 1,
        role: isResult ? "highlight" : "comparison",
        items: [item(r)],
        result: isResult,
        rank: 1,
      });
      if (compareExact(carried, "0") > 0) {
        links.push({ source: total, target: next, role: "highlight", value: carried, owner: next });
      }
      for (const p of pendingIn)
        links.push({ source: p.node, target: next, role: "context", value: p.value, owner: p.node });
      total = next;
      running = r.value as string;
      outflow = "0";
      pendingIn = [];
      continue;
    }
    const effect = effectOf(r.kind as StepKind, r.value as string);
    const sign = compareExact(effect, "0");
    if (sign === 0) continue;
    if (sign < 0) {
      // A cost (or other income that was negative): it leaves the current total.
      const amount = negateExact(effect);
      const n = addNode({
        label: r.label,
        column: column + 1,
        role: "neutral",
        items: [item(r)],
        result: false,
        rank: 2,
      });
      links.push({ source: total, target: n, role: "neutral", value: amount, owner: n });
      outflow = addExact(outflow, amount);
    } else {
      // Other income (or a credit): it joins the next total, from beside the current one.
      if (i > lastTotal) return { ok: false, reason: "trailing-addition" };
      const n = addNode({ label: r.label, column, role: "context", items: [item(r)], result: false, rank: 0 });
      pendingIn.push({ node: n, value: effect });
    }
  }
  return { ok: true, nodes, links, revenue: first.value as string };
}

/**
 * Folds small siblings: segments, the costs leaving one total, or the additions joining one total, under
 * `foldShare` of total revenue, into one "Other" node, when two or more of them are that small (a single small
 * flow keeps its name and is drawn at the minimum thickness). "Other" shows no number: its tooltip lists each line.
 */
export function foldSmall(
  nodes: DraftNode[],
  links: DraftLink[],
  revenue: string,
  share: number = SANKEY.foldShare,
): { nodes: DraftNode[]; links: DraftLink[] } {
  const threshold = Number(revenue) * share;
  // Siblings: leaf nodes with one link, grouped by the node at the other end and by direction.
  const groups = new Map<string, number[]>();
  links.forEach((l, li) => {
    const leafIsSource = l.owner === l.source && l.owner !== l.target;
    const leaf = leafIsSource ? l.source : l.target;
    if (l.role === "highlight") return;
    const degree = links.filter((x) => x.source === leaf || x.target === leaf).length;
    if (degree !== 1 || Number(l.value) >= threshold) return;
    const key = `${leafIsSource ? "in" : "out"}:${leafIsSource ? l.target : l.source}`;
    groups.set(key, [...(groups.get(key) ?? []), li]);
  });
  const drop = new Set<number>();
  const outLinks = [...links];
  const outNodes = [...nodes];
  for (const [key, members] of groups) {
    if (members.length < 2) continue;
    const incoming = key.startsWith("in:");
    const firstLink = links[members[0] as number] as DraftLink;
    const leaves = members.map((li) => {
      const l = links[li] as DraftLink;
      return incoming ? l.source : l.target;
    });
    const leafNodes = leaves.map((n) => nodes[n] as DraftNode);
    const other =
      outNodes.push({
        label: "Other",
        column: (leafNodes[0] as DraftNode).column,
        role: "neutral",
        items: leafNodes.flatMap((n) => n.items),
        folded: true,
        result: false,
        rank: Math.max(...leafNodes.map((n) => n.rank)) + 0.5,
      }) - 1;
    for (const li of members) drop.add(li);
    for (const n of leaves) (outNodes[n] as DraftNode).column = -1;
    const value = sumExact(members.map((li) => (links[li] as DraftLink).value));
    outLinks.push(
      incoming
        ? { source: other, target: firstLink.target, role: firstLink.role, value, owner: other }
        : { source: firstLink.source, target: other, role: firstLink.role, value, owner: other },
    );
  }
  // Remove the folded leaves (column -1) and re-index.
  const keep = outNodes.map((n, i) => (n.column >= 0 ? i : -1)).filter((i) => i >= 0);
  const index = new Map(keep.map((old, i) => [old, i]));
  return {
    nodes: keep.map((i) => outNodes[i] as DraftNode),
    links: outLinks
      .filter((_, li) => !drop.has(li))
      .map((l) => ({
        ...l,
        source: index.get(l.source) as number,
        target: index.get(l.target) as number,
        owner: index.get(l.owner) as number,
      })),
  };
}

/** Lays the graph out: columns, node heights and positions, flow bands, label positions; grows to fit labels. */
export function sankeyLayout(lines: readonly SankeyLineInput[], opts: Partial<typeof SANKEY> = {}): SankeyResult {
  const o = { ...SANKEY, ...opts };
  const graph = sankeyGraph(lines);
  if (!graph.ok) return graph;
  const { nodes: draft, links: draftLinks } = foldSmall(graph.nodes, graph.links, graph.revenue, o.foldShare);
  const columns = Math.max(...draft.map((n) => n.column)) + 1;
  // Each column top to bottom: additions first, then the total, then the costs (and "Other" after its siblings),
  // keeping the given order inside each rank. This keeps the profit path along the top and no two flows crossing.
  const byColumn = Array.from({ length: columns }, (_, c) =>
    draft
      .map((n, i) => ({ n, i }))
      .filter(({ n }) => n.column === c)
      .sort((a, b) => a.n.rank - b.n.rank || a.i - b.i)
      .map(({ i }) => i),
  );

  let height: number = o.height;
  for (let attempt = 0; attempt < 6; attempt++) {
    const placed = place(draft, draftLinks, byColumn, height, o);
    const need = placeLabels(placed.nodes, byColumn, height);
    if (need <= height) return { ok: true, layout: { columns, height, nodes: placed.nodes, links: placed.links } };
    height = Math.ceil(need);
  }
  const placed = place(draft, draftLinks, byColumn, height, o);
  placeLabels(placed.nodes, byColumn, height);
  return { ok: true, layout: { columns, height, nodes: placed.nodes, links: placed.links } };
}

function place(
  draft: readonly DraftNode[],
  draftLinks: readonly DraftLink[],
  byColumn: readonly number[][],
  height: number,
  o: typeof SANKEY,
): { nodes: SankeyNode[]; links: SankeyLink[] } {
  const value = (l: DraftLink) => Number(l.value);
  const thick = (k: number, l: DraftLink) => Math.max(o.minThickness, value(l) * k);
  const nodeHeight = (k: number, i: number) => {
    const ins = draftLinks.filter((l) => l.target === i).reduce((s, l) => s + thick(k, l), 0);
    const outs = draftLinks.filter((l) => l.source === i).reduce((s, l) => s + thick(k, l), 0);
    return Math.max(ins, outs, o.minThickness);
  };
  // The scale: the tallest column, minimum thicknesses included, fits the height with its gaps.
  const columnValue = (col: readonly number[]) =>
    col.reduce((s, i) => {
      const ins = draftLinks.filter((l) => l.target === i).reduce((a, l) => a + value(l), 0);
      const outs = draftLinks.filter((l) => l.source === i).reduce((a, l) => a + value(l), 0);
      return s + Math.max(ins, outs);
    }, 0);
  let k = Math.min(
    ...byColumn.map((col) => (height - (col.length - 1) * o.pad) / Math.max(columnValue(col), Number.MIN_VALUE)),
  );
  for (let i = 0; i < 20; i++) {
    const tallest = Math.max(
      ...byColumn.map((col) => col.reduce((s, n) => s + nodeHeight(k, n), 0) + (col.length - 1) * o.pad),
    );
    if (tallest <= height + 1e-6) break;
    k *= height / tallest;
  }

  const nodes: SankeyNode[] = draft.map((n, i) => ({
    key: `n${i}`,
    label: n.label,
    column: n.column,
    role: n.role,
    items: n.items,
    folded: n.folded,
    result: n.result,
    y: 0,
    height: nodeHeight(k, i),
    labelTop: 0,
  }));
  for (const col of byColumn) {
    let y = 0;
    for (const i of col) {
      const node = nodes[i] as SankeyNode;
      node.y = y;
      y += node.height + o.pad;
    }
  }
  // Bands: stacked at each end in the order of the node at the other end, so flows never cross.
  const links: SankeyLink[] = draftLinks.map((l, li) => ({
    key: `l${li}`,
    source: l.source,
    target: l.target,
    role: l.role,
    value: value(l),
    thickness: thick(k, l),
    y0: 0,
    y1: 0,
    owner: l.owner,
  }));
  nodes.forEach((_, i) => {
    const outs = links.filter((l) => l.source === i).sort((a, b) => yOf(nodes, a.target) - yOf(nodes, b.target));
    let y = (nodes[i] as SankeyNode).y;
    for (const l of outs) {
      l.y0 = y;
      y += l.thickness;
    }
    const ins = links.filter((l) => l.target === i).sort((a, b) => yOf(nodes, a.source) - yOf(nodes, b.source));
    y = (nodes[i] as SankeyNode).y;
    for (const l of ins) {
      l.y1 = y;
      y += l.thickness;
    }
  });
  return { nodes, links };
}

const yOf = (nodes: readonly SankeyNode[], i: number) => (nodes[i] as SankeyNode).y;

/**
 * Stacks labels in one column without overlap (design/charts.md, "Labels"): each starts centred on its mark, then
 * labels are pushed down past the one above, and pulled back up inside the bottom as far as those above allow.
 * Items must be in top-to-bottom order. Returns each label's top and the height the column needs: more than
 * `height` means the plot must grow.
 */
export function stackLabels(
  items: readonly { centre: number; h: number }[],
  height: number,
): { tops: number[]; need: number } {
  const tops = items.map((it) => Math.min(Math.max(it.centre - it.h / 2, 0), Math.max(height - it.h, 0)));
  for (let j = 1; j < items.length; j++) {
    tops[j] = Math.max(tops[j] as number, (tops[j - 1] as number) + (items[j - 1] as { h: number }).h);
  }
  const lastIndex = items.length - 1;
  const last = items[lastIndex];
  if (last && (tops[lastIndex] as number) + last.h > height) {
    tops[lastIndex] = height - last.h;
    for (let j = lastIndex - 1; j >= 0; j--) {
      tops[j] = Math.min(tops[j] as number, (tops[j + 1] as number) - (items[j] as { h: number }).h);
    }
  }
  const need = Math.max(
    height,
    items.reduce((s, it) => s + it.h, 0),
  );
  return { tops: tops.map((t) => Math.max(t, 0)), need };
}

/** Places each node's label beside it, centred, nudged apart within its column. Returns the height they need. */
export function placeLabels(nodes: SankeyNode[], byColumn: readonly number[][], height: number): number {
  let need = height;
  for (const col of byColumn) {
    const column = col.map((i) => nodes[i] as SankeyNode);
    const { tops, need: n } = stackLabels(
      column.map((node) => ({ centre: node.y + node.height / 2, h: labelHeight(node) })),
      height,
    );
    column.forEach((node, j) => {
      node.labelTop = tops[j] as number;
    });
    need = Math.max(need, n);
  }
  return need;
}

/** A flow's band as an SVG path in the 0-1000 × 0-height box: a cubic curve along its top and bottom edges. */
export function flowPath(link: SankeyLink, x0: number, x1: number): string {
  const xm = (x0 + x1) / 2;
  const r = (v: number) => Math.round(v * 100) / 100;
  const t0 = r(link.y0);
  const t1 = r(link.y1);
  const b0 = r(link.y0 + link.thickness);
  const b1 = r(link.y1 + link.thickness);
  return `M${r(x0)} ${t0}C${r(xm)} ${t0} ${r(xm)} ${t1} ${r(x1)} ${t1}L${r(x1)} ${b1}C${r(xm)} ${b1} ${r(xm)} ${b0} ${r(x0)} ${b0}Z`;
}

/** The column's x in the flows' box (0-1000). */
export const columnX = (column: number, columns: number) => (columns <= 1 ? 0 : (column / (columns - 1)) * 1000);

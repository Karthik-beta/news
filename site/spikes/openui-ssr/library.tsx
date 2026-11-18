import { type ComponentRenderer, createLibrary, defineComponent } from "@openuidev/react-lang";
import { z } from "zod/v4";
import { Island } from "./Island";
import type { LineChartProps } from "./LineChart";

// A four-component OpenUI library, just enough for the spike's fixture page. Static components render plain
// semantic HTML; the chart's renderer is swappable so each approach can decide how the island is emitted.

const Heading = defineComponent({
  name: "Heading",
  description: "The page headline.",
  props: z.object({ text: z.string() }),
  component: ({ props }) => <h1 className="oui-heading">{props.text}</h1>,
});

const Paragraph = defineComponent({
  name: "Paragraph",
  description: "A paragraph of reading text.",
  props: z.object({ text: z.string() }),
  component: ({ props }) => <p className="oui-paragraph">{props.text}</p>,
});

const KeyFigure = defineComponent({
  name: "KeyFigure",
  description: "One key figure: label, value, change in words.",
  props: z.object({ label: z.string(), value: z.string(), change: z.string() }),
  component: ({ props }) => (
    <div className="oui-figure">
      <p className="oui-figure-label">{props.label}</p>
      <p className="oui-figure-value">{props.value}</p>
      <p className="oui-figure-change">{props.change}</p>
    </div>
  ),
});

const lineChartProps = z.object({
  title: z.string(),
  subtitle: z.string(),
  labels: z.array(z.string()),
  values: z.array(z.number()),
  source: z.string(),
});

/** Default chart renderer: a server-rendered island (`data-island`) hydrated by hydrate.client.ts. */
export const islandChart: ComponentRenderer<LineChartProps> = ({ props }) => <Island name="LineChart" props={props} />;

export function createSpikeLibrary(chart: ComponentRenderer<LineChartProps> = islandChart) {
  const LineChart = defineComponent({
    name: "LineChart",
    description: "Change over time for one series.",
    props: lineChartProps,
    component: chart,
  });

  const Page = defineComponent({
    name: "Page",
    description: "The page: a list of blocks in reading order.",
    props: z.object({
      children: z.array(z.union([Heading.ref, Paragraph.ref, KeyFigure.ref, LineChart.ref])),
    }),
    component: ({ props, renderNode }) => <article className="oui-page">{renderNode(props.children)}</article>,
  });

  return createLibrary({ components: [Page, Heading, Paragraph, KeyFigure, LineChart], root: "Page" });
}

export const spikeLibrary = createSpikeLibrary();

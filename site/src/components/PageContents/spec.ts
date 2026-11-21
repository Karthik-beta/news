import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "PageContents",
  purpose: 'Page-template part (article rail): "On this page", links to the page\'s own section headings and charts.',
  example: `<PageContents items={[{ id: "why-sales-rose", text: "Why sales rose in {pd_fy2024}" }, { id: "chart", text: chart.title }]} />`,
});

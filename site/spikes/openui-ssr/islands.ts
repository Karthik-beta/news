import { LineChart } from "./LineChart";

// Every component that hydrates in the browser, by name. Server and client both read this registry, so the
// name in the HTML always maps to the same component on both sides. Static components never appear here.
export const islands = { LineChart } as const;

export type IslandName = keyof typeof islands;

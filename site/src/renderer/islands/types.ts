import type { ComponentType } from "react";

// biome-ignore lint/suspicious/noExplicitAny: each island declares its own props; the registry only maps names
export type IslandComponent = ComponentType<any>;

/** Island components by name, each loaded on demand. Generated for production (islands.generated.ts). */
export type IslandLoaders = Record<string, () => Promise<{ default: IslandComponent }>>;

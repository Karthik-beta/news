// Server-renders a tree, then hydrates it in jsdom and reports every React warning or error on the way. Primitives
// are static (they never hydrate on a page), but islands render them too (Tooltip, FigureFrame around a plot), so
// their server and client output must match with no warnings.
import { act, type ReactElement } from "react";
import { hydrateRoot } from "react-dom/client";
import { renderToString } from "react-dom/server";
import { vi } from "vitest";

export async function hydrateAndCollectWarnings(ui: ReactElement): Promise<{ html: string; warnings: string[] }> {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  const html = renderToString(ui);
  const container = document.createElement("div");
  container.innerHTML = html;
  document.body.append(container);
  // Compare against the parsed server HTML (the parser lowercases attribute names such as popoverTarget).
  const parsed = container.innerHTML;
  const warnings: string[] = [];
  const record = (...args: unknown[]) => warnings.push(args.map(String).join(" "));
  const error = vi.spyOn(console, "error").mockImplementation(record);
  const warn = vi.spyOn(console, "warn").mockImplementation(record);
  try {
    const root = await act(async () =>
      hydrateRoot(container, ui, { onRecoverableError: (e) => warnings.push(`recoverable: ${String(e)}`) }),
    );
    const after = container.innerHTML;
    await act(async () => root.unmount());
    if (after !== parsed) warnings.push(`markup changed on hydration:\n${parsed}\n${after}`);
  } finally {
    error.mockRestore();
    warn.mockRestore();
    container.remove();
  }
  return { html, warnings };
}

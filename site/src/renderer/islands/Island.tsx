// Server side of an island (ADR 0001, decision B). The island's subtree is rendered with renderToString as its own
// React root, so its markup is exactly what hydrateRoot produces on the client, wherever it sits in the page. The
// rest of the page is static markup and never hydrates. Props travel as JSON in data-props.
import { type ComponentType, createElement, useId } from "react";
import { renderToString } from "react-dom/server";

/** Island props must survive JSON unchanged: no functions, dates, Maps, class instances or undefined values. */
export function assertJsonProps(name: string, props: unknown): void {
  const seen = (value: unknown, path: string): void => {
    if (value === null || typeof value === "string" || typeof value === "boolean") return;
    if (typeof value === "number") {
      if (!Number.isFinite(value)) throw new TypeError(`Island ${name}: ${path} is ${value}, not JSON`);
      return;
    }
    if (Array.isArray(value)) {
      value.forEach((v, i) => {
        seen(v, `${path}[${i}]`);
      });
      return;
    }
    if (typeof value === "object" && Object.getPrototypeOf(value) === Object.prototype) {
      for (const [k, v] of Object.entries(value)) seen(v, `${path}.${k}`);
      return;
    }
    throw new TypeError(`Island ${name}: ${path} is not plain JSON (${typeof value})`);
  };
  seen(props, "props");
}

export interface IslandProps<P> {
  /** The name in the island registry (the <Name>.client.tsx file name). */
  name: string;
  /** The same component the registry loads on the client. */
  component: ComponentType<P>;
  props: P;
}

/**
 * Renders an island. The wrapper is a block (<div>), so islands sit where blocks may: a chart's plot, not inside
 * a sentence. Each island gets its own useId prefix, so ids never clash between islands or with the page.
 */
export function Island<P extends object>({ name, component, props }: IslandProps<P>) {
  assertJsonProps(name, props);
  const prefix = `i${useId().replace(/[^a-zA-Z0-9]/g, "")}-`;
  const html = renderToString(createElement(component, props), { identifierPrefix: prefix });
  return (
    <div
      className="island"
      data-island={name}
      data-island-prefix={prefix}
      data-props={JSON.stringify(props)}
      // biome-ignore lint/security/noDangerouslySetInnerHtml: React's own server output for the island subtree
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

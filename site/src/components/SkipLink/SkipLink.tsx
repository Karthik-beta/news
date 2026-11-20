// "Skip to content" (design/components.md, "Skip link"): the first focusable element on every page. Hidden until it
// has focus, then shown at the top left in paper on ink. A page-template part: templates put it first in <body>.
export function SkipLink({ target = "main" }: { target?: string }) {
  return (
    <a className="skip-link" href={`#${target}`}>
      Skip to content
    </a>
  );
}

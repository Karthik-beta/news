import { definePrimitive } from "../contract";

export default definePrimitive({
  name: "SkipLink",
  purpose: "Page-template part: 'Skip to content', the first focusable element, shown only on focus.",
  example: `<body><SkipLink target="main" /><Masthead … /><main id="main">…</main></body>`,
});

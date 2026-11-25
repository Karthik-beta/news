/**
 * The renderer version, recorded in every page (meta generator tag, the article's data-renderer attribute,
 * index.md front matter, chart JSON and the render result for meta.json). Bump it when rendered output changes
 * for the same page.oui and data.json, so pages on older versions can be found and re-rendered in batches
 * (Technical Design, "Renderer versions"): major for contract changes (data.json, page.oui), minor for new output,
 * patch for fixes that change markup.
 */
export const RENDERER_VERSION = "1.1.0";
export const RENDERER_NAME = "news-renderer";
export const rendererTag = () => `${RENDERER_NAME} ${RENDERER_VERSION}`;

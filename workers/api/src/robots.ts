// The robots meta tag in a page's index.html (ADR 0004, "Index status"; #56). Article pages are served straight from
// R2 and never run a Worker, so this tag is the page's only per-page indexing switch, and the api keeps it equal to
// the live index_status.
//
// The bundle's HTML is untrusted. It is accepted only with exactly one robots tag, byte for byte one of the two forms
// the renderer writes (site/src/renderer/render.tsx), and the api replaces exactly that tag's characters, nothing
// else. Every `<meta` in the file is read as a tag, even inside a comment, script or text, which can only over-count:
// a meta tag that is not in the plain form the renderer writes (double-quoted values, spaces between attributes), or
// whose name could be encoded, is refused rather than guessed at.
import type { Issue } from "@news/worker-shared/validate";
import { ApiError } from "./errors";

export type IndexStatus = "index" | "noindex";

export const ROBOTS_TAGS: Record<IndexStatus, string> = {
  index: '<meta name="robots" content="index, follow">',
  noindex: '<meta name="robots" content="noindex">',
};

/** `<meta` where an HTML tokenizer ends the tag name (so not `<metadata`), in any case. */
const META_OPEN = /<meta(?=[\t\n\f\r />])/gi;
/** One attribute after whitespace: a name, and optionally `="value"`. A double-quoted value may hold `>` and `<`. */
const ATTR = /[\t\n\f\r ]+([^\t\n\f\r />"'=<`]+)(?:[\t\n\f\r ]*=[\t\n\f\r ]*"([^"]*)")?/y;
const TAG_END = /[\t\n\f\r ]*\/?>/y;

const PATH = "/files/index.html";
const issue = (code: string, message: string): Issue => ({ code, message, path: PATH });

/** Where the one robots tag is, or the issue that says why there is not exactly one the api may rewrite. */
export function findRobotsTag(html: string): { start: number; end: number } | Issue {
  const found: { start: number; end: number }[] = [];
  for (const open of html.matchAll(META_OPEN)) {
    const start = open.index;
    let pos = start + "<meta".length;
    let robots = false;
    for (;;) {
      TAG_END.lastIndex = pos;
      if (TAG_END.test(html)) {
        pos = TAG_END.lastIndex;
        break;
      }
      ATTR.lastIndex = pos;
      const attr = ATTR.exec(html);
      if (!attr)
        return issue("bundle.robots_invalid", `the meta tag at character ${start} is not in the renderer's form`);
      pos = ATTR.lastIndex;
      const name = (attr[1] as string).toLowerCase();
      const value = (attr[2] ?? "").toLowerCase();
      if (name !== "name" && name !== "http-equiv") continue;
      // A character reference could spell "robots"; the renderer never writes one in a meta name.
      if (value.includes("&"))
        return issue(
          "bundle.robots_invalid",
          `the meta tag at character ${start} has a character reference in its ${name}`,
        );
      // robots, googlebot, bingbot, x-robots-tag: any crawler directive counts as a robots tag.
      if (value.includes("bot")) robots = true;
    }
    if (robots) found.push({ start, end: pos });
  }
  const [tag, second] = found;
  if (!tag) return issue("bundle.robots_missing", "index.html has no robots meta tag");
  if (second) return issue("bundle.robots_duplicate", `index.html has ${found.length} robots meta tags; it needs one`);
  const text = html.slice(tag.start, tag.end);
  if (text !== ROBOTS_TAGS.index && text !== ROBOTS_TAGS.noindex)
    return issue("bundle.robots_invalid", `the robots tag must be ${ROBOTS_TAGS.noindex} or ${ROBOTS_TAGS.index}`);
  return tag;
}

/** index.html with its one robots tag set to `status`; no other character changes. */
export function withRobots(html: string, status: IndexStatus): string {
  const tag = findRobotsTag(html);
  if ("code" in tag)
    throw new ApiError(
      "invalid_bundle",
      "index.html has no single robots tag the api can rewrite; republish the page",
      [tag],
    );
  return html.slice(0, tag.start) + ROBOTS_TAGS[status] + html.slice(tag.end);
}

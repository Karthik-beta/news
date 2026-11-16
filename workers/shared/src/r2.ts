// R2 layout: a page's files live under its public path without the leading slash, so the CDN serves
// https://news.karthik.nz/2026/11/apple-fy26-q4-results/index.md straight from R2 key 2026/11/apple-fy26-q4-results/index.md.

/** Content type by file name; the client never chooses it. */
const CONTENT_TYPES: Record<string, string> = {
  "index.html": "text/html; charset=utf-8",
  "index.md": "text/markdown; charset=utf-8",
  "data.json": "application/json; charset=utf-8",
  "sources.json": "application/json; charset=utf-8",
  "meta.json": "application/json; charset=utf-8",
  "page.oui": "text/plain; charset=utf-8",
};

export function contentTypeFor(file: string): string {
  const known = CONTENT_TYPES[file];
  if (known) return known;
  if (/^charts\/[a-z0-9][a-z0-9-]{0,63}\.csv$/.test(file)) return "text/csv; charset=utf-8";
  throw new Error(`no content type for bundle file ${file}`);
}

/** '/2026/11/x' + 'index.md' → '2026/11/x/index.md'. */
export function pageObjectKey(pagePath: string, file: string): string {
  if (!/^\/[0-9]{4}\/[0-9]{2}\/[a-z0-9-]+$/.test(pagePath)) throw new Error(`invalid page path ${pagePath}`);
  if (file.includes("..") || file.startsWith("/")) throw new Error(`invalid bundle file ${file}`);
  return `${pagePath.slice(1)}/${file}`;
}

/** Cache lifetime at the edge for page files; purges on publish keep them fresh. */
export const PAGE_CACHE_CONTROL = "public, max-age=300, s-maxage=86400";

/** Which version of the page a stored file belongs to (R2 custom metadata), so a reader can tell mid-publish. */
export interface FileStamp {
  version: number;
  bundle_sha256: string;
}

export async function putPageFile(
  bucket: R2Bucket,
  pagePath: string,
  file: string,
  body: string,
  stamp: FileStamp,
): Promise<void> {
  await bucket.put(pageObjectKey(pagePath, file), body, {
    httpMetadata: { contentType: contentTypeFor(file), cacheControl: PAGE_CACHE_CONTROL },
    customMetadata: { version: String(stamp.version), bundle_sha256: stamp.bundle_sha256 },
  });
}

/** Deletes the listed files of a page. Missing objects are not an error. */
export async function deletePageFiles(bucket: R2Bucket, pagePath: string, files: string[]): Promise<void> {
  const keys = files.map((f) => pageObjectKey(pagePath, f));
  for (let i = 0; i < keys.length; i += 1000) await bucket.delete(keys.slice(i, i + 1000));
}

export interface CappedText {
  text: string;
  truncated: boolean;
  /** Size of the whole object in bytes. */
  size: number;
  /** The page version the file was written for (its stamp), when it has one. */
  version?: number;
}

/**
 * Reads at most `maxBytes` of an object as UTF-8 (a range read, so a huge object costs no more than the cap).
 * Returns undefined when the object does not exist.
 */
export async function readCapped(bucket: R2Bucket, key: string, maxBytes: number): Promise<CappedText | undefined> {
  const object = await bucket.get(key, { range: { offset: 0, length: maxBytes } });
  if (!object) return undefined;
  const bytes = new Uint8Array(await object.arrayBuffer());
  const truncated = object.size > bytes.length;
  // A cut can split a multi-byte character; the decoder drops the incomplete tail instead of inventing one.
  const text = new TextDecoder("utf-8", { fatal: false, ignoreBOM: false }).decode(bytes, { stream: truncated });
  const version = Number(object.customMetadata?.version);
  return { text, truncated, size: object.size, ...(Number.isInteger(version) && version > 0 ? { version } : {}) };
}

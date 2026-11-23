// Writes the ChoroplethMap's world geometry (design/charts.md, "Maps"): Natural Earth 1:50m admin-0 countries (public
// domain), projected to Equal Earth here, simplified and written as one SVG path per country, so the browser ships no
// projection code and the server only copies strings. Antarctica is left out. Run it only when the geometry changes:
//   curl -LO https://raw.githubusercontent.com/nvkelso/natural-earth-vector/v5.1.2/geojson/ne_50m_admin_0_countries.geojson
//   bun scripts/gen-map-geometry.ts ne_50m_admin_0_countries.geojson
// A unit test (tests/unit/choropleth.test.ts) keeps the committed file within its size budget.
// (curl and the Natural Earth file stay outside the repo; only the output is committed.)
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const site = join(dirname(fileURLToPath(import.meta.url)), "..");
// A module, not JSON: the dynamic route imports no JSON (tests/unit/site-guards.test.ts).
export const OUT = join(site, "src/components/ChoroplethMap/geo/world.ts");
/** The viewBox width; heights follow the projection. */
const WIDTH = 1000;
/** Douglas-Peucker tolerance and the smallest island kept (in viewBox units; 1 unit is about 0.7px at full width). */
const TOLERANCE = 0.6;
const MIN_AREA = 2;

type Ring = [number, number][];

/** Equal Earth (Šavrič, Patterson and Jenny 2018), unit sphere, central meridian 0; y up. */
export function equalEarth(lon: number, lat: number): [number, number] {
  const [A1, A2, A3, A4] = [1.340264, -0.081106, 0.000893, 0.003796];
  const M = Math.sqrt(3) / 2;
  const l = (lon * Math.PI) / 180;
  const t = Math.asin(M * Math.sin((lat * Math.PI) / 180));
  const t2 = t * t;
  const t6 = t2 * t2 * t2;
  const x = (l * Math.cos(t)) / (M * (A1 + 3 * A2 * t2 + t6 * (7 * A3 + 9 * A4 * t2)));
  const y = t * (A1 + A2 * t2 + t6 * (A3 + A4 * t2));
  return [x, y];
}

function simplify(ring: Ring, tol: number): Ring {
  if (ring.length <= 4) return ring;
  const keep = new Uint8Array(ring.length);
  // A ring is closed (its ends are one point), so it is split at the point farthest from its start.
  const [sx, sy] = ring[0] as [number, number];
  let mid = 1;
  ring.forEach(([x, y], i) => {
    const [mx, my] = ring[mid] as [number, number];
    if (Math.hypot(x - sx, y - sy) > Math.hypot(mx - sx, my - sy)) mid = i;
  });
  keep[0] = keep[mid] = keep[ring.length - 1] = 1;
  const stack: [number, number][] = [
    [0, mid],
    [mid, ring.length - 1],
  ];
  while (stack.length) {
    const [a, b] = stack.pop() as [number, number];
    const [ax, ay] = ring[a] as [number, number];
    const [bx, by] = ring[b] as [number, number];
    const len = Math.hypot(bx - ax, by - ay) || 1;
    let far = -1;
    let dist = tol;
    for (let i = a + 1; i < b; i++) {
      const [px, py] = ring[i] as [number, number];
      const d = Math.abs((bx - ax) * (ay - py) - (ax - px) * (by - ay)) / len;
      if (d > dist) [far, dist] = [i, d];
    }
    if (far > 0) {
      keep[far] = 1;
      stack.push([a, far], [far, b]);
    }
  }
  return ring.filter((_, i) => keep[i]);
}

const area = (r: Ring) =>
  Math.abs(
    r.reduce((s, [x, y], i) => s + x * (r[(i + 1) % r.length]?.[1] ?? 0) - (r[(i + 1) % r.length]?.[0] ?? 0) * y, 0),
  ) / 2;

/** One ring as "M x y l dx dy … z", in whole viewBox units (rounded absolutely, so no error builds up). */
function ringPath(ring: Ring): string {
  const pts = ring.map(([x, y]) => [Math.round(x), Math.round(y)] as const);
  let out = "";
  let prev: readonly [number, number] | null = null;
  for (const p of pts) {
    if (prev === null) out += `M${p[0]} ${p[1]}l`;
    else if (p[0] !== prev[0] || p[1] !== prev[1]) out += `${p[0] - prev[0]} ${p[1] - prev[1]} `;
    else continue;
    prev = p;
  }
  // A ring smaller than one unit rounds to a single point: draw it as a one-unit square, a speck on the map.
  if (out.endsWith("l")) return `${out.slice(0, -1)}h1v1h-1z`;
  return `${out.trimEnd().replace(/ -/g, "-")}z`;
}

if (import.meta.main) {
  const input = process.argv[2];
  if (!input) throw new Error("usage: bun scripts/gen-map-geometry.ts <ne_50m_admin_0_countries.geojson>");
  const geo = JSON.parse(readFileSync(input, "utf8")) as {
    features: { properties: Record<string, string>; geometry: { type: string; coordinates: unknown } }[];
  };
  const features = geo.features.filter((f) => f.properties.ADM0_A3 !== "ATA");
  const projected = features.map((f) => {
    const p = f.properties;
    // ISO 3166-1 alpha-3 where one exists; Natural Earth's own code otherwise (Kosovo KOS, Somaliland SOL).
    const code = p.ISO_A3 && p.ISO_A3 !== "-99" ? p.ISO_A3 : (p.ADM0_A3 as string);
    const polys = (f.geometry.type === "Polygon" ? [f.geometry.coordinates] : f.geometry.coordinates) as Ring[][];
    return { code, rings: polys.flat().map((r) => r.map(([lon, lat]) => equalEarth(lon, lat))) };
  });
  const all = projected.flatMap((f) => f.rings.flat());
  const [minX, maxX] = [Math.min(...all.map((p) => p[0])), Math.max(...all.map((p) => p[0]))];
  const [minY, maxY] = [Math.min(...all.map((p) => p[1])), Math.max(...all.map((p) => p[1]))];
  const k = WIDTH / (maxX - minX);
  const height = Math.ceil((maxY - minY) * k);
  const regions: Record<string, string> = {};
  for (const f of projected.sort((a, b) => a.code.localeCompare(b.code))) {
    if (regions[f.code]) throw new Error(`two features with the code ${f.code}`);
    const rings = f.rings
      .map((r) =>
        simplify(
          r.map(([x, y]) => [(x - minX) * k, (maxY - y) * k] as [number, number]),
          TOLERANCE,
        ),
      )
      .sort((a, b) => area(b) - area(a));
    // Every country keeps its largest ring, however small, so it is on the map (if only as a speck).
    const kept = rings.filter((r, i) => i === 0 || (r.length >= 4 && area(r) >= MIN_AREA));
    regions[f.code] = kept.map(ringPath).join("");
  }
  const out = {
    source: "Natural Earth 1:50m Cultural Vectors, Admin 0 – Countries, v5.1.2 (naturalearthdata.com)",
    licence: "Public domain; no permission needed (naturalearthdata.com/about/terms-of-use)",
    projection: "Equal Earth, central meridian 0; Antarctica left out; simplified for screen",
    viewBox: `0 0 ${WIDTH} ${height}`,
    regions,
  };
  const type =
    "{ source: string; licence: string; projection: string; viewBox: string; regions: Record<string, string> }";
  const header = "// GENERATED by site/scripts/gen-map-geometry.ts from Natural Earth (public domain). Do not edit.";
  writeFileSync(OUT, `${header}\nexport const world: ${type} = ${JSON.stringify(out, null, 2)};\n`);
  Bun.spawnSync(["bunx", "biome", "format", "--write", OUT], { stdout: "inherit", stderr: "inherit" });
  const bytes = Buffer.byteLength(JSON.stringify(out));
  console.log(`${Object.keys(regions).length} regions, ${(bytes / 1024).toFixed(1)} KB, viewBox ${out.viewBox}`);
}

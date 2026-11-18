// Copies the self-hosted font files into site/public/fonts/ from the pinned Fontsource packages (design/typography.md,
// "Loading the fonts"). The site never loads fonts from Google or any other third party. Only the subsets the site
// needs are copied: Latin, Latin Extended (€ £ ₹ ₩ and the rest of our currency signs) and Greek (µ σ Δ for
// science pages). src/styles/fonts.css declares them with unicode-range, so a browser downloads only what a page uses.
// The copies are git-ignored; this runs before dev, build and test. Never edit them by hand.
import { copyFileSync, existsSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const site = join(dirname(fileURLToPath(import.meta.url)), "..");
const repo = join(site, "..");
const target = join(site, "public/fonts");

export const SUBSETS = ["latin", "latin-ext", "greek"] as const;

/** Every file the site ships: [package folder, file name]. fonts.css references exactly these. */
export function fontFiles(): [string, string][] {
  const files: [string, string][] = [];
  for (const subset of SUBSETS) {
    // Literata variable with both axes the design uses (wght 400-800, opsz 7-72), roman and italic.
    for (const style of ["normal", "italic"]) {
      files.push(["@fontsource-variable/literata", `literata-${subset}-opsz-${style}.woff2`]);
    }
    // Fira Sans static: 400, 500, 600, 700 roman and 400 italic.
    for (const weight of [400, 500, 600, 700])
      files.push(["@fontsource/fira-sans", `fira-sans-${subset}-${weight}-normal.woff2`]);
    files.push(["@fontsource/fira-sans", `fira-sans-${subset}-400-italic.woff2`]);
  }
  return files;
}

if (import.meta.main) {
  if (existsSync(target)) rmSync(target, { recursive: true });
  mkdirSync(target, { recursive: true });
  for (const [pkg, file] of fontFiles()) {
    const from = join(repo, "node_modules", pkg, "files", file);
    if (!existsSync(from)) {
      console.error(`Missing font file ${from}. Run \`bun install\`.`);
      process.exit(1);
    }
    copyFileSync(from, join(target, file));
  }
  console.log(`${readdirSync(target).length} font files synced to site/public/fonts/`);
}

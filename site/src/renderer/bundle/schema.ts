// The page bundle's data contract: data.json (every value a page references) and sources.json (where each value
// came from). These zod schemas are the single definition; contracts/bundle/*.schema.json are generated from them
// (`bun run contracts` in site/) and a test fails if the committed JSON Schemas drift. The Rust CLI writes bundles
// against the JSON Schemas; the renderer validates every bundle against these before rendering.
//
// Values are decimal strings exactly as the source published them, never floats, and are never converted:
// currency, unit, scale and period are recorded as stated. See site/src/renderer/README.md.
import { z } from "zod/v4";

/** The six reference kinds the page writer may use (News — Bureau Design, §3.8 "Reference formats"). */
export const REF_KINDS = ["dp", "dv", "sr", "pd", "q", "cx"] as const;
export type RefKind = (typeof REF_KINDS)[number];

/** A whole reference: lower-case `prefix_body`, `_`-separated parts. */
export const REF_PATTERN = /^(dp|dv|sr|pd|q|cx)_[a-z0-9]+(_[a-z0-9]+)*$/;
export const refPattern = (kind: RefKind) => new RegExp(`^${kind}_[a-z0-9]+(_[a-z0-9]+)*$`);
/** Source IDs are page-local and use the same shape with the `src` prefix. */
export const SOURCE_ID_PATTERN = /^src_[a-z0-9]+(_[a-z0-9]+)*$/;

const ref = (kind: RefKind) =>
  z
    .string()
    .regex(refPattern(kind))
    .meta({ description: `A ${kind}_… reference.` });
export const SourceId = z.string().regex(SOURCE_ID_PATTERN).meta({ description: "A page-local source ID, src_…." });

export const Decimal = z
  .string()
  .regex(/^-?(0|[1-9][0-9]*)(\.[0-9]+)?$/)
  .meta({ description: "An exact decimal, as a string: optional minus, no exponent, no grouping." });

const IsoDate = z
  .string()
  .regex(/^\d{4}-\d{2}-\d{2}$/)
  .meta({ description: "A calendar date, YYYY-MM-DD." });
const IsoDateTime = z
  .string()
  .regex(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(:\d{2}(\.\d+)?)?(Z|[+-]\d{2}:\d{2})$/)
  .meta({ description: "A date-time with an offset (RFC 3339)." });

export const Scale = z
  .enum(["units", "thousands", "millions", "billions", "trillions"])
  .meta({ description: "The scale the source states the value in. The value is NOT multiplied by it." });

const Currency = z
  .string()
  .regex(/^[A-Z]{3}$/)
  .meta({ description: "ISO 4217 code when the value is money. Never converted." });

const Locator = z
  .object({ kind: z.string().min(1) })
  .catchall(z.unknown())
  .meta({
    description:
      "Where the value sits inside the stored source document: XBRL tag and context, JSON path, PDF page and table, SDMX key.",
  });

/** dp_…: a data point exactly as the source published it. */
export const DataPoint = z
  .object({
    label: z.string().min(1).meta({ description: "The source's own label, verbatim." }),
    value: Decimal.nullable().meta({
      description: "Exact decimal parsed from value_text; null when the source did not publish a value.",
    }),
    value_text: z.string().nullable().meta({ description: "The value exactly as published ('1,234.5', '-0.3')." }),
    unit: z.string().min(1).meta({ description: "Verbatim unit ('USD', 'Percent', 'Index 1982-84=100')." }),
    currency: Currency.nullable(),
    scale: Scale.nullable(),
    decimals: z
      .number()
      .int()
      .nullable()
      .optional()
      .meta({ description: "Digits after the point the source is precise to (XBRL @decimals), in the value's scale." }),
    period: ref("pd"),
    seasonal_adjustment: z.enum(["SA", "NSA", "SAAR"]).nullable().optional(),
    source: SourceId,
    locator: Locator,
    supersedes: ref("dp").nullable().optional(),
  })
  .strict();

/** dv_…: a value calculated by code from data points, with the formula recorded ("Calculated by News from …"). */
export const DerivedValue = z
  .object({
    label: z.string().min(1),
    value: Decimal,
    unit: z
      .string()
      .min(1)
      .meta({ description: "'%' for pct_change and share, 'pp' for pp_change, else the inputs' unit." }),
    currency: Currency.nullable(),
    scale: Scale.nullable(),
    formula: z.enum(["pct_change", "pp_change", "diff", "share"]),
    inputs: z.array(ref("dp")).min(1),
    rounding: z.number().int().min(0).meta({ description: "Digits after the point the value was rounded to." }),
  })
  .strict();

/** sr_…: an ordered series of data points. A null point is a gap (the source has no value for that period). */
export const Series = z
  .object({
    label: z.string().min(1),
    unit: z.string().min(1),
    currency: Currency.nullable(),
    scale: Scale.nullable(),
    points: z
      .array(z.object({ period: ref("pd"), point: ref("dp").nullable() }).strict())
      .min(1)
      .meta({ description: "In time order. `point: null` marks a missing observation." }),
  })
  .strict();

/** pd_…: a period label exactly as the source writes it ("fiscal 2024", "October 2026", "Q4 FY26"). */
export const Period = z
  .object({
    label: z.string().min(1),
    start: IsoDate.nullable().optional(),
    end: IsoDate.nullable().optional(),
    fiscal: z.boolean().optional().meta({ description: "True when the label is the company's own fiscal period." }),
  })
  .strict();

/** q_…: a quote, an exact substring of a stored text block. */
export const Quote = z
  .object({
    text: z.string().min(1),
    source: SourceId,
    speaker: z.string().nullable().optional(),
    role: z.string().nullable().optional(),
    said_on: IsoDate.nullable().optional(),
    locator: Locator,
  })
  .strict();

/** cx_…: a context line computed by code ("Lowest 12-month rate since March 2021"), with the rule that made it. */
export const ContextLine = z
  .object({
    text: z.string().min(1),
    rule: z.string().min(1),
    inputs: z.array(z.string().regex(REF_PATTERN)).optional(),
  })
  .strict();

export const PageData = z
  .object({
    $schema: z.string().optional(),
    version: z.literal(1),
    points: z.record(ref("dp"), DataPoint).meta({ description: "dp_… data points." }),
    derived: z.record(ref("dv"), DerivedValue).meta({ description: "dv_… derived values." }),
    series: z.record(ref("sr"), Series).meta({ description: "sr_… series." }),
    periods: z.record(ref("pd"), Period).meta({ description: "pd_… period labels." }),
    quotes: z.record(ref("q"), Quote).meta({ description: "q_… quotes." }),
    context: z.record(ref("cx"), ContextLine).meta({ description: "cx_… context lines." }),
  })
  .strict()
  .meta({
    title: "News page bundle: data.json",
    description: "Every value a page references, exactly as the source states it, keyed by reference.",
  });

export const SourceType = z.enum([
  "filing",
  "dataset",
  "press_release",
  "statement",
  "report",
  "paper",
  "preprint",
  "news",
  "other",
]);

export const Source = z
  .object({
    title: z.string().min(1).meta({ description: "The document: 'Apple Inc. Form 10-K, fiscal 2024'." }),
    short: z.string().min(1).meta({ description: "Short form for source lines: 'Form 10-K, fiscal 2024'." }),
    publisher: z.string().min(1).meta({ description: "'Apple Inc.', 'U.S. Bureau of Labor Statistics'." }),
    connector: z.string().min(1).meta({ description: "The connector that fetched it ('sec-edgar', 'fred')." }),
    type: SourceType,
    primary: z.boolean().meta({ description: "False for context sources such as news outlets." }),
    url: z.url({ protocol: /^https?$/ }),
    licence: z
      .object({
        name: z.string().min(1).meta({ description: "'Public filing', 'CC BY 4.0'." }),
        url: z.url().nullable().optional(),
        redistribution: z
          .boolean()
          .meta({ description: "False: the page may cite but not offer the data as CSV/JSON." }),
      })
      .strict(),
    published_at: IsoDate.nullable().optional(),
    retrieved_at: z.union([IsoDate, IsoDateTime]),
  })
  .strict();

export const PageSources = z
  .object({
    $schema: z.string().optional(),
    version: z.literal(1),
    sources: z.record(SourceId, Source),
  })
  .strict()
  .meta({
    title: "News page bundle: sources.json",
    description: "The source records a page's data points and quotes point at.",
  });

export type DataPointT = z.infer<typeof DataPoint>;
export type DerivedValueT = z.infer<typeof DerivedValue>;
export type SeriesT = z.infer<typeof Series>;
export type PeriodT = z.infer<typeof Period>;
export type QuoteT = z.infer<typeof Quote>;
export type ContextLineT = z.infer<typeof ContextLine>;
export type PageDataT = z.infer<typeof PageData>;
export type SourceT = z.infer<typeof Source>;
export type PageSourcesT = z.infer<typeof PageSources>;

// The lead story (design/components.md, "Lead story"): the home page's first slot. Fly title, display headline,
// summary (lead-summary), a meta line (page type, region and date, Fira ink-3), then the slot's mini chart or key
// figure under a brand rule with its tag. All text is the page record's own; the slot names only the page and,
// optionally, which of its figures or charts to show. A page not in the listings renders nothing (the template
// back-fills the slot). Static: 0 KB of JavaScript.
import { formatShortDate } from "../../format";
import { useListings } from "../../renderer/data/listings";
import { FlyTitle } from "../FlyTitle/FlyTitle";
import { Headline } from "../Headline/Headline";
import { ruleTop } from "../Rule/Rule";
import { SlotFigure } from "../StoryItem/SlotFigure";
import { listingMeta } from "../StoryItem/StoryItem";

export interface LeadStoryProps {
  pageId: string;
  figure?: string | null;
  chart?: string | null;
  /** The headline's level: 2 on the home page, under its hidden h1. */
  level?: 2 | 3;
}

export function LeadStory({ pageId, figure, chart, level = 2 }: LeadStoryProps) {
  const page = useListings().get(pageId);
  if (!page) return null;
  const r = page.record;
  return (
    <article className="lead-story">
      {r.fly_title ? <FlyTitle text={r.fly_title} plain /> : null}
      <Headline text={r.title} size="display" level={level} href={r.path} plain />
      <p className="lead-story__summary text-lead-summary">{r.summary}</p>
      <p className="lead-story__meta">
        {listingMeta(r)}, <time dateTime={r.published_at}>{formatShortDate(r.published_at)}</time>
      </p>
      {figure || chart ? (
        <div className={`lead-story__figure ${ruleTop("brand")}`}>
          <SlotFigure page={page} figure={figure} chart={chart} size="m" />
        </div>
      ) : null}
    </article>
  );
}

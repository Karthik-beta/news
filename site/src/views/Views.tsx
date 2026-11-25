// The templates of the pages that list pages (design/layout.md: home, category hubs, entity hubs, Latest, 404). Each is
// one React tree rendered on the server with no JavaScript, so the page components can share one <ListingsProvider>.
// Every word of a story is a page record's; the words here are the template's own (design/content-style.md).
import type { ReactNode } from "react";
import { DateLine } from "../components/DateLine/DateLine";
import { FlyTitle } from "../components/FlyTitle/FlyTitle";
import { Headline } from "../components/Headline/Headline";
import { LatestList } from "../components/LatestList/LatestList";
import { LeadStory } from "../components/LeadStory/LeadStory";
import { NotFound } from "../components/NotFound/NotFound";
import { RegionFilter, regionHref } from "../components/RegionFilter/RegionFilter";
import { SearchField } from "../components/SearchField/SearchField";
import { SectionHead } from "../components/SectionHead/SectionHead";
import { Select } from "../components/Select/Select";
import { StateMessage } from "../components/StateMessage/StateMessage";
import { SlotFigure } from "../components/StoryItem/SlotFigure";
import { StoryItem, StoryList } from "../components/StoryItem/StoryItem";
import { TextField } from "../components/TextField/TextField";
import type { Slot } from "../home/compose";
import { ListingsProvider, PageListings } from "../renderer/data/listings";
import type { Body } from "../server/views";
import { listedOf, SEARCH_LIMIT } from "../server/views";

type HomeBody = Extract<Body, { type: "home" | "region" }>;
type HubBody = Extract<Body, { type: "hub" }>;
type SearchBody = Extract<Body, { type: "search" }>;

/** "Older": the only way on in a cursor list. The pagination component's markup, with one link (ADR 0010). */
function Older({ href }: { href: string | undefined }) {
  if (!href) return null;
  return (
    <nav className="pagination" aria-label="Pages">
      <ul className="pagination__list">
        <li>
          <a className="pagination__end" href={href} rel="next">
            Older
          </a>
        </li>
      </ul>
    </nav>
  );
}

const story = (s: Slot, size: "s" | "m" = "s") => (
  <StoryItem
    key={s.story.page_id}
    pageId={s.story.page_id}
    figure={s.figure}
    chart={s.chart}
    variant="home"
    size={size}
    level={3}
  />
);

function Home({ body }: { body: HomeBody }) {
  const { model } = body;
  const region = body.type === "region";
  const regionOf = (slug: string) => (slug === "all" ? "/" : `/region/${encodeURIComponent(slug)}`);
  return (
    <>
      <h1 className="visually-hidden">
        {region ? `News from ${body.regions.find((r) => r.slug === body.selected)?.label}` : "Front page"}
      </h1>
      <DateLine date={body.date}>
        <RegionFilter selected={body.selected} regions={body.regions} href={regionOf} />
      </DateLine>
      {model.lead ? (
        <div className="home-lead">
          <div className="home-lead__main">
            <LeadStory
              pageId={model.lead.story.page_id}
              figure={model.lead.figure}
              chart={model.lead.chart}
              level={2}
            />
          </div>
          {model.side.length > 0 ? (
            <div className="home-lead__side">
              {model.side.map((s) => (
                <div key={s.story.page_id} className="home-lead__story">
                  {story(s, "m")}
                </div>
              ))}
            </div>
          ) : null}
        </div>
      ) : null}
      {model.latest.length > 0 ? (
        <LatestList pageIds={model.latest.map((e) => e.page_id)} now={body.now} allHref="/latest" />
      ) : null}
      {model.sections.map((section) => (
        <section
          key={section.category}
          className="home-section"
          aria-label={body.names[section.category] ?? (section.category || "More stories")}
        >
          {section.category ? (
            <SectionHead name={body.names[section.category] ?? section.category} href={`/${section.category}`} />
          ) : (
            <div className="home-section__rule" />
          )}
          <div className="home-section__row">{section.slots.map((s) => story(s))}</div>
        </section>
      ))}
      {region ? <Older href={body.olderHref} /> : null}
      {!model.lead && model.latest.length === 0 ? (
        <StateMessage
          message={region ? "There are no pages for this region yet." : "There are no pages yet."}
          action={{ href: region ? "/" : "/latest", label: region ? "Back to the front page" : "See the latest pages" }}
        />
      ) : null}
    </>
  );
}

function Hub({ body }: { body: HubBody }) {
  const strip = body.strip;
  return (
    <>
      <header className="hub-head">
        {body.fly ? <FlyTitle text={body.fly} plain /> : null}
        <Headline text={body.title} size="xl" level={1} plain />
        {body.subcategories.length > 0 ? (
          <nav aria-label="Sub-categories">
            <ul className="hub-head__subs">
              {body.subcategories.map((s) => (
                <li key={s.href}>
                  <a href={s.href}>{s.label}</a>
                </li>
              ))}
            </ul>
          </nav>
        ) : null}
      </header>
      {body.filter ? (
        <RegionFilter
          selected={body.filter.selected}
          regions={body.filter.regions}
          href={(slug) => regionHref(body.filter?.base ?? "/", slug)}
        />
      ) : null}
      {strip ? (
        <section className="hub-strip" aria-label="Latest figures">
          <h2 className="hub-strip__label text-label">Latest figures</h2>
          <ul className="hub-strip__list">
            {strip.figureIds.map((id) => (
              <li key={id}>
                <SlotFigure page={strip.page} figure={id} />
              </li>
            ))}
          </ul>
          <p className="hub-strip__from">
            From <a href={strip.page.record.path}>{strip.page.record.title}</a>
          </p>
        </section>
      ) : null}
      {body.entries.length === 0 ? (
        <StateMessage
          message={
            body.filter?.selected !== "all" ? "There are no pages in this region yet." : "There are no pages here yet."
          }
          action={
            body.filter?.selected !== "all"
              ? { href: body.filter?.base ?? "/", label: "Show all regions" }
              : { href: "/latest", label: "See the latest pages" }
          }
        />
      ) : body.years ? (
        body.years.map((y) => (
          <section key={y.year} className="hub-year" aria-labelledby={`year-${y.year}`}>
            <h2 className="hub-year__label text-headline-l" id={`year-${y.year}`}>
              {y.year}
            </h2>
            <StoryList pageIds={y.entries.map((e) => e.page_id)} variant="hub" level={3} />
          </section>
        ))
      ) : (
        <StoryList pageIds={body.entries.map((e) => e.page_id)} variant="hub" level={2} />
      )}
      <Older href={body.olderHref} />
    </>
  );
}

function Search({ body }: { body: SearchBody }) {
  const options = (list: SearchBody["regions"]) => list.map((o) => ({ value: o.slug, label: o.label }));
  const f = body.filters;
  return (
    <>
      <header className="hub-head">
        <Headline text="Search" size="xl" level={1} plain />
      </header>
      <SearchField
        action="/search"
        query={body.query}
        filters={
          <>
            <Select
              id="search-category"
              name="category"
              label="Category"
              options={options(body.categories)}
              value={f.category}
            />
            <Select id="search-region" name="region" label="Region" options={options(body.regions)} value={f.region} />
            <Select id="search-type" name="type" label="Page type" options={options(body.types)} value={f.type} />
            <TextField id="search-from" name="from" type="date" label="From" value={f.from} />
            <TextField id="search-to" name="to" type="date" label="To" value={f.to} />
          </>
        }
      />
      {body.unavailable ? (
        <StateMessage
          message="Search has a daily limit on the free database it runs on, and today's limit has been reached. It returns after 00:00 UTC. You can still browse the categories or the latest pages."
          action={{ href: "/latest", label: "See the latest pages" }}
        />
      ) : body.problem ? (
        <StateMessage message={body.problem} />
      ) : !body.searched ? null : body.hits.length === 0 ? (
        <StateMessage
          message={`No pages match "${body.query}". Try fewer words, or remove a filter.`}
          action={
            // design/components.md "Empty state": with filters set, the way on is to clear them.
            Object.values(f).some(Boolean)
              ? { href: `/search?q=${encodeURIComponent(body.query)}`, label: "Clear the filters" }
              : { href: "/latest", label: "See the latest pages" }
          }
        />
      ) : (
        <>
          <p className="text-label" role="status">
            {body.hits.length === 1 ? "1 page" : `${body.hits.length} pages`}
            {body.hits.length === SEARCH_LIMIT
              ? ". Showing the best matches: add a word or a filter to narrow the search."
              : ""}
          </p>
          <StoryList pageIds={body.hits.map((h) => h.record.page_id)} variant="hub" level={2} />
        </>
      )}
    </>
  );
}

/** The listings every list component in the tree reads. */
function listingsOf(body: Body): PageListings {
  if (body.type === "notfound") return new PageListings([]);
  if (body.type === "search") return new PageListings(body.hits);
  if (body.type === "hub") return new PageListings(body.entries.map((e) => listedOf(e)));
  return new PageListings(body.pages);
}

export function Views({ body }: { body: Body }): ReactNode {
  return (
    <ListingsProvider listings={listingsOf(body)}>
      {body.type === "notfound" ? (
        <NotFound searchAction="/search" />
      ) : body.type === "hub" ? (
        <Hub body={body} />
      ) : body.type === "search" ? (
        <Search body={body} />
      ) : (
        <Home body={body} />
      )}
    </ListingsProvider>
  );
}

// Builder's sanity checks for the #7 family in jsdom: every component fixture page passes axe in both themes, the
// video island loads only youtube-nocookie.com and only after the click, the gallery is links before hydration,
// and the template parts mark the current page. Cases in fixtures/cases.tsx are checked by primitives.test.tsx.
// The formal tests are the component tester's.
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import ImageGalleryIsland from "../../src/components/ImageGallery/ImageGallery.client";
import { Masthead } from "../../src/components/Masthead/Masthead";
import { RegionFilter } from "../../src/components/RegionFilter/RegionFilter";
import VideoEmbedIsland from "../../src/components/VideoEmbed/VideoEmbed.client";
import { componentFixtures, renderComponentFixture } from "../../src/renderer/fixtures/component-pages";
import { expectNoAxeViolations, renderComponent } from "../support/render";

describe("component fixture pages", () => {
  it.each(componentFixtures.map((f) => [`${f.component}/${f.fixture}`, f] as const))(
    "%s has no axe violations (light and dark)",
    async (_name, f) => {
      const { article } = renderComponentFixture(f.component, f.fixture);
      for (const theme of ["light", "dark"] as const) {
        // biome-ignore lint/security/noDangerouslySetInnerHtml: the renderer's own static output under test
        const { container, unmount } = renderComponent(<main dangerouslySetInnerHTML={{ __html: article }} />, {
          theme,
        });
        await expectNoAxeViolations(container);
        unmount();
      }
    },
    60_000, // axe over the twelve-image gallery takes several seconds on a loaded laptop
  );
});

describe("VideoEmbed island", () => {
  const props = {
    youtubeId: "aBcDeFgHiJk",
    title: "FOMC press conference",
    url: "https://www.federalreserve.gov/",
  };

  it("loads the youtube-nocookie.com player only after the click, at the same size, and focuses it", () => {
    const { container } = render(<VideoEmbedIsland {...props} />);
    expect(container.querySelector("iframe")).toBeNull();
    expect(container.innerHTML).not.toMatch(/youtube/);
    fireEvent.click(screen.getByRole("button", { name: "Load video from YouTube: FOMC press conference" }));
    const frame = container.querySelector("iframe");
    expect(frame?.getAttribute("src")).toBe("https://www.youtube-nocookie.com/embed/aBcDeFgHiJk?autoplay=1");
    expect(frame?.getAttribute("title")).toBe("FOMC press conference");
    expect(frame?.parentElement?.className).toBe("video-embed__frame");
    expect(document.activeElement).toBe(frame);
  });
});

describe("ImageGallery island", () => {
  const image = (n: number) => ({
    src: `/fixtures/images/crater-1000x1000.svg?${n}`,
    alt: `Image ${n}.`,
    width: 1000,
    height: 1000,
    credit: "NASA",
    licence: "Public domain",
    caption: null,
  });

  it("each image is a link to the full-size file, so it opens without JavaScript", () => {
    render(<ImageGalleryIsland images={[image(1), image(2)]} crop={false} labelledBy={null} />);
    const links = screen.getAllByRole("link");
    expect(links.map((l) => l.getAttribute("href"))).toEqual([image(1).src, image(2).src]);
    expect(screen.getByRole("link", { name: "Image 1." })).toBeInTheDocument();
  });
});

describe("navigation parts mark the current page", () => {
  it("Masthead and RegionFilter set aria-current on the current link only", () => {
    render(
      <>
        <Masthead current="/finance/" />
        <RegionFilter selected="india" />
      </>,
    );
    const current = document.querySelectorAll('[aria-current="page"]');
    expect([...current].map((a) => a.textContent)).toEqual(["Finance", "India"]);
  });
});

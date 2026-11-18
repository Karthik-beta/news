// The client entry for fixture pages (built only with NEWS_FIXTURES=1): hydrates production and fixture islands.
import { hydrateIslands } from "../islands/hydrate";
import { fixtureIslands } from "./islands";

hydrateIslands(fixtureIslands);

// PUT and DELETE /__preview/curation: injected by the `news-preview` integration only for NEWS_PREVIEW=1 builds.
import { env } from "cloudflare:workers";
import type { APIRoute } from "astro";
import type { SiteEnv } from "../../data";
import { handleDraft } from "../../preview";

export const prerender = false;
export const PUT: APIRoute = ({ request }) => handleDraft(request, env as SiteEnv);
export const DELETE: APIRoute = ({ request }) => handleDraft(request, env as SiteEnv);

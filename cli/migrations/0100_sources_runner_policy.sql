-- Contacts (Bureau §3.1), read by the Runner from `sources` after `news sources sync` copies them
-- from `data/sources/<id>.toml`:
-- * requires_contact_email: put NEWS_CONTACT_EMAIL in this source's User-Agent (SEC);
-- * robots_exempt: URL prefixes where robots.txt is not applied although robots_respect is on.

alter table sources add column if not exists requires_contact_email boolean not null default false;
alter table sources add column if not exists robots_exempt text[] not null default '{}';

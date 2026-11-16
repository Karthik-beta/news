-- Contacts (Bureau §3.1): whether the Runner must put NEWS_CONTACT_EMAIL in this source's User-Agent
-- (SEC). Read from `data/sources/<id>.toml` by `news sources sync`, not inferred from the connector.

alter table sources add column if not exists requires_contact_email boolean not null default false;

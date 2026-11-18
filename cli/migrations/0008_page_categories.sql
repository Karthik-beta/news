-- Pages in categories (TD "Data model": `taxonomy`, "pages link to many categories"; "Taxonomy that can
-- grow": a page can belong to several categories and sub-categories). A link table, so a category can be
-- added, split or renamed without touching pages, and hubs list a category's pages from its index.

create table if not exists page_categories (
  page_id text not null references pages,
  tax_id  bigint not null references taxonomy,
  primary key (page_id, tax_id)
);
create index if not exists page_categories_tax on page_categories (tax_id, page_id);

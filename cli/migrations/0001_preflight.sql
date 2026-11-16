-- Core block 0001–0099 (cli/README.md, "Migrations"). Every statement is idempotent: re-running a file
-- changes nothing. Tables are unqualified: `news db migrate` sets search_path to the target schema
-- (`news` by default; a throwaway schema in tests), with `public` behind it for the extensions.

-- The extensions are created once per database by infra/postgres/init/001_extensions.sql (a superuser
-- step); migrations only check for them, so a test schema never gets its own copy.
do $$
begin
  if not exists (select 1 from pg_extension where extname = 'vector') then
    raise exception 'extension "vector" is missing: apply infra/postgres/init/001_extensions.sql';
  end if;
  if not exists (select 1 from pg_extension where extname = 'pg_trgm') then
    raise exception 'extension "pg_trgm" is missing: apply infra/postgres/init/001_extensions.sql';
  end if;
end
$$;

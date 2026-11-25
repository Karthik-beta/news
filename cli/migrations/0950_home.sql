-- Home commands (#20; ADR 0005). The curator's model travels with the draft into curation.v1.
alter table home_curations add column if not exists curator_model text;

# pipeline

Settings and inputs for the `claude -p` page calls (Bureau Design §3.9; Technical Design, "Keeping each claude -p call lean").

## Prompt prefixes

Fixer (`cli/crates/news-fixer`, `prefix` module) builds one cached prefix per page type: `prefix/reporter-instructions.md`, then `skills/news-brand`, then that page type's `skills/page-*` skill, then the component index (`news components`) and the specs of the components the skill's "Typical layout" names. The text is written to `<work>/prefix/<page-type>.<hash12>.md`, named by the SHA-256 of its bytes, so every call of a page type shares one file and prompt caching stays warm. The instructions and skills are compiled into the `news` binary: change them, rebuild, and the hash changes.

`prefix/reporter-instructions.md` is a draft; its final wording is written in #19.

## Briefs

The brief Reporter reads is `brief.v1` (`contracts/cli/ai/brief.v1.schema.json`; Rust type `news_core::brief::Brief`). `news brief <story>` prints it; the `item.brief` job writes `<work>/briefs/<story>.json` and `<story>.refs.json` (what each ref resolves to). Briefs aim for 3,000 tokens and are refused above 4,000, estimated as UTF-8 bytes / 3.

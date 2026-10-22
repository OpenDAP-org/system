# ADR 0007: Licensing of language data

- **Status:** Accepted
- **Date:** 2026-10-22
- **Decided by:** Claude, under the maintainers' delegation ("choose the best standard")

## Context

The UI and keyboard ship in English, Mandarin Chinese, Hindi, Spanish and Arabic (SYS-027, SYS-028), with language data loaded progressively (SYS-029). That data (locale data, translations, keyboard dictionaries, frequency lists, prediction models, input-method tables) comes from many sources under different licences. Some good word lists are GPL-2.0-only (Hindi) or GPL-3.0 (rime-ice), which do not mix freely with each other or with every part of the platform ([text input and i18n research](../research/2026-10-text-input-and-i18n.md)).

## Decision

1. **Locale data follows the Unicode standard:** CLDR through ICU4X, under the Unicode License v3.
2. **Preferred licences for other language data, in order:** CC0 / public domain; permissive (Unicode License, MIT, BSD, Apache-2.0); CC-BY-4.0; CC-BY-SA-4.0 (the project's own documentation licence).
3. **Copyleft software licences (GPL, LGPL) on data are accepted only for separately distributed language packs**, never embedded in the base image or in a component under another licence. Such a pack carries its own licence file and source, and is optional.
4. **Every language pack declares its licence and origin** in REUSE form (SPDX identifier, copyright, source URL and version), and the pack builder rejects data without a declared licence.
5. **Our own language data** (translations of OpenDAP strings, our layouts and dictionaries derived from preferred-licence sources) is CC-BY-SA-4.0, like the rest of the project's assets.

## Consequences

- The default image stays free of copyleft data obligations beyond the platform's own licences.
- For a language whose best data is GPL-only, the pack is still available, as a separate optional download.
- The pack format (SYS-029) gains a mandatory licence and origin manifest.

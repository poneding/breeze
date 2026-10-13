# Automatic Language Detection and an English-Source Catalog

## Request

The user asked for three changes on October 13, 2026:

- Add automatic detection to the interface-language dropdown. Detection maps
  Simplified and Traditional Chinese locales to the matching catalog, and every
  other locale to English.
- Order the dropdown as detection, a divider, then English, 简体中文, 繁體中文.
- Use English as the catalog key and the code's default language, as
  [AGENTS.md](../../AGENTS.md) requires for code and documentation.

## Design

`Language` (English, Simplified Chinese, Traditional Chinese) is the concrete
rendering language and never reaches the catalog as "auto". `LanguagePreference`
(0 Auto, 1 English, 2 Simplified Chinese, 3 Traditional Chinese) is the stored
choice and the Select item. `LanguagePreference::resolve` maps Auto through the
operating-system locale and every explicit choice to its language.

`language_for_locale` lowercases the OS tag. A tag that is `zh`, `zh-*` or `zh_*`
is Traditional Chinese when it carries `hant`, `-tw`/`_tw`, `-hk`/`_hk` or
`-mo`/`_mo`; otherwise it is Simplified Chinese. Everything else, including a
missing tag, is English. `sys-locale` (`=0.3.2`, already in the lockfile through
`cosmic-text`) supplies the tag on macOS, Windows and Linux.

The catalog is `"English source" => ("简体中文", "繁體中文")`. `text_in` returns
the English key for English and for any language it does not carry, so the source
language is the fallback rather than a separate entry. Four pairs of near-synonym
Chinese keys shared one English meaning and are merged into one entry each.

The dropdown is one ungrouped Select over `LanguageItem` rows: `Auto detect`, a
non-selectable `Languages` label row, then the three explicit languages. The label
is a row rather than a section header because a Kit Select measures only its first
section header and applies that height to every header, so any header — a divider
rule or a title — would reserve an equally tall, empty band above the first option.
The label row is disabled (muted, no hover) and committing it keeps the current
language, so it can never become the selected value.

Review iterations rejected both a plain divider and the `Detection`/`Languages`
group titles the theme selector uses: the divider had to be as tall as the empty
band above detection, and `Detection` only named a single option.

## Constraints

- Detection only selects among the three existing catalogs; it adds no language.
- Application-owned chrome changes language; filenames, user categories, document
  text and official theme names do not.
- Tests must not depend on the machine's locale: under `cfg(test)` Auto resolves
  to Simplified Chinese, while `language_for_locale` is unit-tested directly.
- The stored choice survives the index shift. State version 9 migrates version 1–8
  snapshots (0/1/2 = Simplified Chinese/English/Traditional Chinese) onto the new
  indices, so an existing user keeps the language they chose. Auto is the default
  only for a fresh state.
- No PDF content, annotation, or document-sidecar behavior changes.

## Verification

`cargo fmt --all -- --check`, the workspace suite and
`cargo clippy --workspace --all-targets --locked -- -D warnings` pass. New tests
cover locale matching, preference round-tripping and detection, the dropdown's
rows and their positions, the label row's veto, the version 8 → 9 language migration, English-source
catalog key uniqueness and slot parity, and live chrome refresh through the real
Select.

Native macOS checks used the isolated review bundle under
`target/lang-detection-review/`. Because the screen-recording permission is not
granted to this session, the open dropdown could not be captured as an image; the
verification was done through the application's accessibility tree and window
title instead. Auto detection rendered Simplified Chinese on this Simplified-Chinese
system and English when launched with an `en-US` language preference; a stored
English or Traditional Chinese preference rendered `Breeze · Local reading
workspace` and `Breeze · 本地閱讀工作台`, and the English chrome listed Home, Open
document, Change theme, Settings, All documents, Recent, Favorites, New category,
Unassigned, Shortcuts, About, List view and Grid view. The dropdown's rendered
order and label row therefore rest on component tests rather than a capture.
Windows and Linux native checks remain unperformed.

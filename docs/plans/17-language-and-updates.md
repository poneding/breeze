# Interface languages and release checks

Requested on October 11, 2026. This work contains two separately verified phases.

1. Replace the language preview with a native Select for English, Simplified Chinese,
   and Traditional Chinese. Translate application chrome, tools, prompts and notices;
   preserve user content, filenames, document text, and official theme names. Apply
   changes immediately, refresh existing input placeholders and theme groups, and
   restore the saved choice. Retain numeric preference compatibility (0 = Simplified
   Chinese, 1 = English, 2 = Traditional Chinese).
2. Query stable GitHub Releases for `poneding/breeze`, the future repository named by
   the user. Implement manual checks, an optional startup check, semantic version
   comparison, bounded background requests and clear failure/empty states. A newer
   release opens its verified GitHub release page. Downloading, installing, creating
   the remote repository, and publishing are outside this request.

The repository does not yet exist publicly. Unavailable repository and absent stable
release responses must not be represented as an up-to-date result. Update policy is
persisted; Manual remains the default. Never disables background checks while an
explicit manual check remains available.

Each phase requires formatting, workspace tests, all-target Clippy, isolated native
startup and visual inspection where available. Tests must exercise translation
coverage, live Select changes and persisted restoration; update fixtures must cover
version ordering, HTTP failures, bounds and concurrent/stale requests. All review
state, screenshots and scratch outputs stay under `target/`.

## Implemented behavior

Both phases are implemented. The update client checks GitHub's latest stable release
endpoint, accepts valid SemVer tags (optional v/V prefix), uses a 15-second total
deadline and 1 MiB response limit, and never follows redirects. It constructs release
links under the fixed repository and does not interpret server-supplied HTML/URLs.
Automatic checks run once per session at startup or first enablement; explicit checks
remain available after errors and under every policy. See STATUS for executed checks
and native/platform limits.

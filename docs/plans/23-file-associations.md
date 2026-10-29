# Work Package 23: Opening With Breeze from the OS

Status: implementation extended from PDF to every readable format on October
20, 2026. Windows and Linux native registration checks require their own hosts;
macOS is verified from the local review bundle.

## Request

On October 15, 2026 the user asked that supported files — PDF first — can be
opened directly with Breeze from the operating system's context menu ("Open
With" / "打开方式"). On October 20, 2026 the user asked that EPUB and Markdown
files open with Breeze as well.

## Scope

Opening a document with Breeze has two halves that must both work before the
feature is real:

1. **Registration.** Every distributed Breeze install declares that it can
   *view* each readable format, so Finder, File Explorer and Linux desktop file
   managers list Breeze in "Open With". Registration never claims the system
   default: the user makes that choice through the operating system.
2. **Delivery.** The launched or already-running application receives the
   requested document, instead of dropping it or opening an empty library.

An OS-owned *default* app choice, signing, and installer verification stay
outside this package.

## Registration per platform

- macOS (`scripts/bundle-macos.sh`, `scripts/release.py`): `CFBundleDocumentTypes`
  entries with role `Viewer` and `LSHandlerRank` `Alternate` for
  `com.adobe.pdf` (`.pdf`), `org.idpf.epub-container` (`.epub`) and
  `net.daringfireball.markdown` (`.md`, `.markdown`). `Alternate` keeps Breeze
  in the Finder menu without silently taking over the default handler.
- Windows (`scripts/release.py`): `Applications\breeze-app.exe` with
  `SupportedTypes` for `.pdf`, `.epub`, `.md` and `.markdown`, plus the
  `Breeze.Document`, `Breeze.Epub` and `Breeze.Markdown` ProgIDs registered in
  their extensions' `OpenWithProgids` keys. Both the MSI and the setup
  executable uninstall the same keys, and Inno requests an association-change
  notification so Explorer refreshes.
- Linux (`scripts/release.py`):
  `MimeType=application/pdf;application/epub+zip;text/markdown;` in the shared
  desktop entry. The entry already passes `%F`, so a file manager can hand over
  one or more documents.

Registration follows the formats `DocumentFormat` accepts. It still never
writes an extension's default value, and the existing `Breeze.Document` ProgID
stays the PDF type so upgrades keep the same registration.

## Delivery per platform

- Command-line arguments stay the Windows/Linux path: every non-flag argument
  is a document request.
- macOS delivers `application:openURLs:` to the running process, which may occur
  before the reader window exists. The platform callback has no application
  context, so requests are queued and handed to the window once it is created;
  later requests are delivered through the window on the foreground executor.
- File URLs are decoded (`file://` scheme, percent escapes); non-file URLs are
  ignored rather than guessed at.
- A launch that cannot take the state lock (another Breeze instance owns the
  data directory) still opens the requested document and reports the storage
  warning, instead of silently discarding the request.
- A document requested while the library is still restoring waits for the
  restore, because restore replaces the whole book list.

## Verification

- Unit tests for URL decoding, argument filtering, and the delivery queue.
- Component test: a queued request opens a tab once restore completes.
- `scripts/test_release.py` asserts the PDF, EPUB and Markdown registrations in
  the macOS plist, the WiX and Inno registry entries, and the Linux desktop
  entry.
- Native macOS check: build the review bundle, register it, confirm the three
  claims in the Launch Services database, and open a PDF, an EPUB and a
  Markdown document from the command line into a running instance.
- Windows and Linux native "Open With" checks remain outstanding and must be
  reported as unverified.

# Work Package 23: Opening With Breeze from the OS

Status: implementation in progress after this plan was written. Windows and
Linux native registration checks require their own hosts; macOS is verified
from the local review bundle.

## Request

On October 15, 2026 the user asked that supported files — PDF first — can be
opened directly with Breeze from the operating system's context menu ("Open
With" / "打开方式").

## Scope

Opening a PDF with Breeze has two halves that must both work before the feature
is real:

1. **Registration.** Every distributed Breeze install declares that it can
   *view* PDF documents, so Finder, File Explorer and Linux desktop file
   managers list Breeze in "Open With". Registration never claims the system
   default: the user makes that choice through the operating system.
2. **Delivery.** The launched or already-running application receives the
   requested document, instead of dropping it or opening an empty library.

An OS-owned *default* app choice, signing, and installer verification stay
outside this package.

## Registration per platform

- macOS (`scripts/bundle-macos.sh`, `scripts/release.py`): `CFBundleDocumentTypes`
  with role `Viewer`, `LSHandlerRank` `Alternate`, and the `com.adobe.pdf`
  content type. `Alternate` keeps Breeze in the Finder menu without silently
  taking over the default handler.
- Windows (`scripts/release.py`): `Applications\breeze-app.exe` with
  `SupportedTypes` `.pdf` and a shell `open` command, plus a
  `Breeze.Document` ProgID registered in `.pdf\OpenWithProgids`. Both the MSI
  and the setup executable uninstall the same keys, and Inno requests an
  association-change notification so Explorer refreshes.
- Linux (`scripts/release.py`): `MimeType=application/pdf;` in the shared
  desktop entry. The entry already passes `%F`, so a file manager can hand over
  one or more documents.

The installer only declares PDF: Breeze's product constraint is PDF as the
first backend, and the reader rejects other formats today.

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
- `scripts/test_release.py` asserts the PDF registration in the macOS plist,
  the WiX and Inno registry entries, and the Linux desktop entry.
- Native macOS check: build the review bundle, register it, confirm the PDF
  claim in the Launch Services database, and open a PDF from the command line
  into a running instance.
- Windows and Linux native "Open With" checks remain outstanding and must be
  reported as unverified.

#!/usr/bin/env python3
"""Validate release tags and package prebuilt binaries as platform installers.

Python 3.11+, standard library only. Assembling the payload is platform
independent, but building each container needs that platform's own tooling, so
`package` must run on the matching operating system:

* macOS: `hdiutil` wraps `Breeze.app` in a disk image.
* Windows: WiX (`candle`, `light`) builds the MSI and Inno Setup (`iscc`) builds
  the setup executable.
* Linux: `dpkg-deb` builds the Debian package and `appimagetool` the AppImage.

None of these tools are application runtime dependencies, and none of them sign
or notarize the result. Every artifact is an unsigned development package.

The installers carry the same payload the archives used to: the executable (or
`Breeze.app`), `VERSION`, `LICENSE`, `Cargo.lock`, the repository README, these
release notes, a corresponding-source pointer, and the maintained third-party
notices under `licenses/`.
"""

import argparse
import hashlib
import os
from pathlib import Path
import plistlib
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
import uuid
from xml.sax.saxutils import quoteattr

ROOT = Path(__file__).resolve().parent.parent
# SemVer 2.0.0: numeric prerelease identifiers cannot have leading zeroes;
# build identifiers can. Git tag convention permits one optional lowercase v.
NUMBER = r"(?:0|[1-9][0-9]*)"
PRE = rf"(?:{NUMBER}|[0-9]*[A-Za-z-][0-9A-Za-z-]*)"
SEMVER = re.compile(
    rf"(?P<core>{NUMBER}\.{NUMBER}\.{NUMBER})"
    rf"(?:-(?P<pre>{PRE}(?:\.{PRE})*))?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
)

# One container per platform. Linux and Windows ship two each so a user can
# pick a package manager or a plain setup program.
FORMATS: dict[str, tuple[str, ...]] = {
    "aarch64-apple-darwin": ("dmg",),
    "x86_64-apple-darwin": ("dmg",),
    "x86_64-pc-windows-msvc": ("msi", "exe"),
    "x86_64-unknown-linux-gnu": ("deb", "appimage"),
}
TARGETS = tuple(FORMATS)

APP_NAME = "Breeze"
DISPLAY_NAME = "Breeze"
EXECUTABLE = "Breeze"
LINUX_EXECUTABLE = "breeze-app"
# PDF registration. Breeze declares that it can *view* this type so every file
# manager lists it under "Open with"; none of these entries take the default
# handler over, because only the user may change that through the OS.
PDF_CONTENT_TYPE = "com.adobe.pdf"
PDF_PROGID = "Breeze.Document"
WINDOWS_PDF_EXTENSION = ".pdf"
WINDOWS_EXECUTABLE = "breeze-app.exe"
# Linux names the same format through the shared MIME database.
PDF_MIME_TYPE = "application/pdf"
LINUX_PACKAGE = "breeze"
PUBLISHER = "Pone Ding <poneding@gmail.com>"
HOMEPAGE = "https://github.com/poneding/breeze"
# Stable namespace so the same version rebuilds byte-identical identifiers and
# a new version reuses the Windows upgrade/tie-to-product codes.
GUID_NAMESPACE = uuid.UUID("6d1f0e0b-6f8a-5c3a-9c1e-3b7a2d4f6e10")
# Ubuntu 24.04 renamed some libraries during the 64-bit time_t transition.
LINUX_DEPENDS = (
    "libc6 (>= 2.39)",
    "libfontconfig1",
    "libfreetype6",
    "libx11-6",
    "libxkbcommon0",
    "libwayland-client0",
    "libxcb1",
    "libvulkan1",
    "libudev1",
    "libasound2 | libasound2t64",
    "libglib2.0-0 | libglib2.0-0t64",
)
# Package managers and setup builders are external tools; these are the install
# directories to check when they are not on PATH.
WINDOWS_TOOL_DIRS = (
    Path(r"C:/Program Files (x86)/WiX Toolset v3.14/bin"),
    Path(r"C:/Program Files (x86)/WiX Toolset v3.11/bin"),
    Path(r"C:/Program Files (x86)/Inno Setup 6"),
)
# Finder and Explorer metadata must never reach an installer payload: a stray
# `.DS_Store` would ship inside the package and get published as a release file.
METADATA = shutil.ignore_patterns(".DS_Store", "._*", "Thumbs.db", "desktop.ini")


def tag_metadata(tag: str, manifest: Path) -> dict[str, str]:
    version = tag.removeprefix("v")
    match = SEMVER.fullmatch(version)
    if not match:
        return {"valid": "false"}
    with manifest.open("rb") as source:
        expected = tomllib.load(source)["workspace"]["package"]["version"]
    if version != expected:
        raise ValueError(f"tag version {version!r} does not match Cargo.toml {expected!r}")
    return {
        "valid": "true",
        "version": version,
        "prerelease": str(match["pre"] is not None).lower(),
    }


def _version_parts(version: str) -> re.Match:
    match = SEMVER.fullmatch(version)
    if not match:
        raise ValueError(f"not a SemVer version: {version!r}")
    return match


def _address_tool(name: str, extra_dirs: tuple[Path, ...] = ()) -> str:
    """Resolve a packaging tool, preferring PATH and then known install dirs."""
    found = shutil.which(name)
    if found:
        return found
    for directory in extra_dirs:
        for candidate in (directory / name, directory / f"{name}.exe"):
            if candidate.is_file():
                return str(candidate)
    raise ValueError(f"packaging tool not found: {name}")


def _identifier(prefix: str, value: str) -> str:
    """Build a deterministic WiX identifier that is unique per input."""
    cleaned = re.sub(r"[^0-9A-Za-z_]", "_", value).strip("_")[:40]
    digest = hashlib.sha256(value.encode("utf-8")).hexdigest()[:8]
    return f"{prefix}_{cleaned}_{digest}"


def _guid(*parts: str) -> str:
    return "{" + str(uuid.uuid5(GUID_NAMESPACE, "-".join(parts))).upper() + "}"


def _attr(value: object) -> str:
    """Quote an XML attribute value, escaping XML-significant characters."""
    return quoteattr(str(value))


def _is_metadata(name: str) -> bool:
    return name in {".DS_Store", "Thumbs.db", "desktop.ini"} or name.startswith("._")


def _debian_version(version: str) -> str:
    """Map SemVer onto the Debian version rules.

    Debian sorts `~` before the empty string, so a prerelease keeps its SemVer
    position (before the matching stable release) instead of sorting after it.
    """
    match = _version_parts(version)
    if match["pre"]:
        return f"{match['core']}~{match['pre']}-1"
    return f"{match['core']}-1"


def _write_bytes(path: Path, data: bytes, executable: bool = False) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    if executable:
        path.chmod(0o755)


def _copy(path: Path, destination: Path, executable: bool = False) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(path, destination)
    if executable:
        destination.chmod(0o755)


def _write_documents(destination: Path, version: str) -> None:
    """Write the provenance files every installer carries."""
    _write_bytes(destination / "VERSION", (version + "\n").encode("utf-8"))
    _copy(ROOT / "LICENSE", destination / "LICENSE")
    _copy(ROOT / "Cargo.lock", destination / "Cargo.lock")
    _copy(ROOT / "README.md", destination / "README.md")
    _copy(ROOT / "docs/releases.md", destination / "RELEASE-NOTES.md")
    source_info = (
        f"Corresponding source for Breeze {version} is published with this version's release at:\n"
        "https://github.com/poneding/breeze/releases\n\n"
        f"Download Breeze-{version}-source.tar.gz. It contains the exact Breeze source, Cargo.lock,\n"
        "build/release scripts, and vendored locked Cargo source packages. The archive can be\n"
        "built offline with the Rust, C/C++, and libclang toolchains documented in\n"
        "the repository development guide (docs/development.md).\n\n"
        "For a workflow_dispatch dry run, the source archive is provided as the separate\n"
        "breeze-source workflow artifact and is not published to a GitHub Release.\n"
    )
    _write_bytes(destination / "SOURCE-CODE.txt", source_info.encode("utf-8"))
    licenses = destination / "licenses"
    if licenses.exists():
        shutil.rmtree(licenses)
    shutil.copytree(ROOT / "assets/licenses", licenses, ignore=METADATA)


def desktop_entry() -> str:
    """The Linux desktop entry shared by the deb and the AppImage.

    `MimeType` declares the PDF documents Breeze can view, which is what makes
    file managers offer it under "Open with". The desktop environment's own
    chooser still owns the default handler. `%F` passes one or more selected
    files, matching the command-line arguments the application accepts.
    """
    return (
        "[Desktop Entry]\n"
        "Type=Application\n"
        f"Name={DISPLAY_NAME}\n"
        "Comment=Read documents locally\n"
        f"Exec={LINUX_EXECUTABLE} %F\n"
        f"Icon={LINUX_PACKAGE}\n"
        "Terminal=false\n"
        "Categories=Office;Viewer;\n"
        f"MimeType={PDF_MIME_TYPE};\n"
    )


def debian_control(version: str, payload: Path) -> str:
    installed = sum(
        path.stat().st_size for path in payload.rglob("*") if path.is_file()
    )
    installed_kib = (installed + 1023) // 1024
    return (
        f"Package: {LINUX_PACKAGE}\n"
        f"Version: {_debian_version(version)}\n"
        "Section: office\n"
        "Priority: optional\n"
        "Architecture: amd64\n"
        f"Maintainer: {PUBLISHER}\n"
        f"Homepage: {HOMEPAGE}\n"
        f"Installed-Size: {installed_kib}\n"
        f"Depends: {', '.join(LINUX_DEPENDS)}\n"
        "Description: Native cross-format reading application\n"
        " Breeze is a native, cross-format reading application. This package\n"
        " contains an unsigned development build that has not passed native\n"
        " runtime acceptance; see /usr/share/doc/breeze/RELEASE-NOTES.md.\n"
    )


def wix_source(version: str, target: str, payload: Path, icon: Path) -> str:
    """Generate the WiX source for the MSI.

    Windows Installer versions are numeric, so the product version carries the
    SemVer core and the full version travels in a property, exactly as the
    macOS bundle splits its two version fields.
    """
    core = _version_parts(version)["core"]
    product = _guid("product", target, version)
    upgrade = _guid("upgrade", target)
    description = f"{DISPLAY_NAME} {version} (unsigned development build)"
    comments = "Unsigned development build; native runtime acceptance is pending."
    lines = [
        '<?xml version="1.0" encoding="UTF-8"?>',
        '<Wix xmlns="http://schemas.microsoft.com/wix/2006/wi">',
        f"  <Product Id={_attr(product)} Name={_attr(DISPLAY_NAME)} Language=\"1033\" "
        f"Version={_attr(core)} Manufacturer={_attr(PUBLISHER)} "
        f"UpgradeCode={_attr(upgrade)}>",
        '    <Package InstallerVersion="500" Compressed="yes" '
        'InstallScope="perMachine" Platform="x64" '
        f"Description={_attr(description)} />",
        '    <MajorUpgrade DowngradeErrorMessage="A newer version of Breeze is '
        'already installed." />',
        '    <MediaTemplate EmbedCab="yes" />',
        f'    <Property Id="BREEZERELEASEVERSION" Value={_attr(version)} Secure="yes" />',
        f'    <Property Id="ARPCOMMENTS" Value={_attr(comments)} />',
        '    <Property Id="ARPNOMODIFY" Value="1" />',
        f'    <Icon Id="BreezeIcon" SourceFile={_attr(icon)} />',
        '    <Property Id="ARPPRODUCTICON" Value="BreezeIcon" />',
        '    <Condition Message="Breeze requires Windows 10 or newer.">Installed OR '
        "VersionNT &gt;= 1000</Condition>",
        '    <Directory Id="TARGETDIR" Name="SourceDir">',
        '      <Directory Id="ProgramFiles64Folder">',
        '        <Directory Id="INSTALLFOLDER" Name="Breeze">',
    ]

    components: list[str] = []

    def emit(directory: Path, relative: Path, depth: int) -> None:
        pad = "  " * depth
        # One component per file: a Windows Installer component may carry exactly
        # one key path, so a shared component could not mark every file as its
        # own key path.
        files = sorted(
            path
            for path in directory.iterdir()
            if path.is_file() and not _is_metadata(path.name)
        )
        for file in files:
            name = file.relative_to(payload).as_posix()
            component = _identifier("cmp", f"{target}/{name}")
            file_id = _identifier("fil", f"{target}/{name}")
            lines.append(
                f"{pad}<Component Id={_attr(component)} "
                f"Guid={_attr(_guid('component', target, name))}>"
            )
            lines.append(
                f"{pad}  <File Id={_attr(file_id)} Source={_attr(file)} "
                'KeyPath="yes" />'
            )
            lines.append(f"{pad}</Component>")
            components.append(component)
        for child in sorted(path for path in directory.iterdir() if path.is_dir()):
            name = child.relative_to(payload).as_posix()
            directory_id = _identifier("dir", f"{target}/{name}")
            lines.append(
                f"{pad}<Directory Id={_attr(directory_id)} Name={_attr(child.name)}>"
            )
            emit(child, child.relative_to(payload), depth + 1)
            lines.append(f"{pad}</Directory>")

    emit(payload, Path("."), 5)

    # Shell registration. Both components keep a registry value as their key
    # path, so Windows Installer removes the PDF association on uninstall
    # without touching the user's chosen default application.
    shortcut = _identifier("cmp", f"{target}/shortcut")
    progid = _identifier("cmp", f"{target}/pdf-progid")
    open_with = _identifier("cmp", f"{target}/pdf-open-with")
    command = f'"[INSTALLFOLDER]{WINDOWS_EXECUTABLE}" "%1"'
    lines.extend(
        [
            f"          <Component Id={_attr(progid)} "
            f"Guid={_attr(_guid('registry', target, 'pdf-progid'))}>",
            f'            <RegistryValue Root="HKLM" '
            f'Key={_attr(f"Software\\Classes\\{PDF_PROGID}")} Type="string" '
            f'Value="Breeze PDF Document" KeyPath="yes" />',
            f'            <RegistryValue Root="HKLM" '
            f'Key={_attr(f"Software\\Classes\\{PDF_PROGID}\\DefaultIcon")} '
            f'Type="string" Value={_attr(f"[INSTALLFOLDER]{WINDOWS_EXECUTABLE},0")} />',
            f'            <RegistryValue Root="HKLM" '
            f'Key={_attr(f"Software\\Classes\\{PDF_PROGID}\\shell\\open\\command")} '
            f'Type="string" Value={_attr(command)} />',
            "          </Component>",
            f"          <Component Id={_attr(open_with)} "
            f"Guid={_attr(_guid('registry', target, 'pdf-open-with'))}>",
            f'            <RegistryValue Root="HKLM" '
            f'Key={_attr(f"Software\\Classes\\{WINDOWS_PDF_EXTENSION}\\OpenWithProgids")} '
            f'Name={_attr(PDF_PROGID)} Type="string" Value="" KeyPath="yes" />',
            f'            <RegistryValue Root="HKLM" '
            f'Key={_attr(f"Software\\Classes\\Applications\\{WINDOWS_EXECUTABLE}\\SupportedTypes")} '
            f'Name={_attr(WINDOWS_PDF_EXTENSION)} Type="string" Value="" />',
            f'            <RegistryValue Root="HKLM" '
            f'Key={_attr(f"Software\\Classes\\Applications\\{WINDOWS_EXECUTABLE}")} '
            f'Name="FriendlyAppName" Type="string" Value={_attr(DISPLAY_NAME)} />',
            f'            <RegistryValue Root="HKLM" '
            f'Key={_attr(f"Software\\Classes\\Applications\\{WINDOWS_EXECUTABLE}\\shell\\open\\command")} '
            f'Type="string" Value={_attr(command)} />',
            "          </Component>",
            "        </Directory>",
            "      </Directory>",
            '      <Directory Id="ProgramMenuFolder">',
            '        <Directory Id="ApplicationProgramsFolder" Name="Breeze">',
            f"          <Component Id={_attr(shortcut)} "
            f"Guid={_attr(_guid('shortcut', target))}>",
            f"            <Shortcut Id=\"ApplicationStartMenuShortcut\" "
            f"Name={_attr(DISPLAY_NAME)} "
            f"Description={_attr(f'{DISPLAY_NAME} {version}')} "
            'Target="[INSTALLFOLDER]breeze-app.exe" WorkingDirectory="INSTALLFOLDER" />',
            '            <RemoveFolder Id="RemoveApplicationProgramsFolder" '
            'Directory="ApplicationProgramsFolder" On="uninstall" />',
            '            <RegistryValue Root="HKCU" '
            'Key="Software\\Breeze" Name="installed" Type="integer" Value="1" '
            'KeyPath="yes" />',
            "          </Component>",
            "        </Directory>",
            "      </Directory>",
            "    </Directory>",
            '    <Feature Id="Main" Title="Breeze" Level="1">',
        ]
    )
    for component in components:
        lines.append(f'      <ComponentRef Id="{component}" />')
    lines.append(f'      <ComponentRef Id="{shortcut}" />')
    lines.append(f'      <ComponentRef Id="{progid}" />')
    lines.append(f'      <ComponentRef Id="{open_with}" />')
    lines.extend(["    </Feature>", "  </Product>", "</Wix>", ""])
    return "\n".join(lines)


def inno_script(version: str, target: str, payload: Path, icon: Path, output: Path) -> str:
    """Generate the Inno Setup script for the Windows setup executable.

    Inno treats a single `{` as a constant reference, so a literal braced GUID
    needs the documented `{{GUID}` spelling, and the AppId must stay identical
    across versions for an upgrade to replace an installed copy. The registry
    block matches the MSI's shell registration: Breeze appears under "Open
    with" and is never installed as the PDF default handler.
    """
    core = _version_parts(version)["core"]
    app_id = "{{" + str(uuid.uuid5(GUID_NAMESPACE, f"inno-{target}")).upper() + "}"
    executable = r"{app}\breeze-app.exe"
    command = f'""{executable}"" ""%1""'
    classes = r"Software\Classes"
    registry = [
        f'Root: HKLM; Subkey: "{classes}\\{PDF_PROGID}"; ValueType: string; '
        f'ValueName: ""; ValueData: "Breeze PDF Document"; Flags: uninsdeletekey',
        f'Root: HKLM; Subkey: "{classes}\\{PDF_PROGID}\\DefaultIcon"; '
        f'ValueType: string; ValueName: ""; ValueData: "{executable},0"; '
        "Flags: uninsdeletekey",
        f'Root: HKLM; Subkey: "{classes}\\{PDF_PROGID}\\shell\\open\\command"; '
        f'ValueType: string; ValueName: ""; ValueData: "{command}"; '
        "Flags: uninsdeletekey",
        f'Root: HKLM; Subkey: "{classes}\\{WINDOWS_PDF_EXTENSION}\\OpenWithProgids"; '
        f'ValueType: string; ValueName: "{PDF_PROGID}"; ValueData: ""; '
        "Flags: uninsdeletevalue",
        f'Root: HKLM; Subkey: "{classes}\\Applications\\{WINDOWS_EXECUTABLE}"; '
        f'ValueType: string; ValueName: "FriendlyAppName"; ValueData: "{DISPLAY_NAME}"; '
        "Flags: uninsdeletekey",
        f'Root: HKLM; Subkey: "{classes}\\Applications\\{WINDOWS_EXECUTABLE}\\SupportedTypes"; '
        f'ValueType: string; ValueName: "{WINDOWS_PDF_EXTENSION}"; ValueData: ""; '
        "Flags: uninsdeletevalue",
        f'Root: HKLM; Subkey: "{classes}\\Applications\\{WINDOWS_EXECUTABLE}\\shell\\open\\command"; '
        f'ValueType: string; ValueName: ""; ValueData: "{command}"; '
        "Flags: uninsdeletekey",
    ]
    return (
        "[Setup]\n"
        f"AppId={app_id}\n"
        f"AppName={DISPLAY_NAME}\n"
        f"AppVersion={version}\n"
        f"AppVerName={DISPLAY_NAME} {version}\n"
        f"AppPublisher={PUBLISHER}\n"
        f"AppPublisherURL={HOMEPAGE}\n"
        f"VersionInfoVersion={core}\n"
        f"VersionInfoDescription={DISPLAY_NAME} {version} (unsigned development build)\n"
        "DefaultDirName={autopf}\\Breeze\n"
        "DefaultGroupName=Breeze\n"
        "DisableProgramGroupPage=yes\n"
        "AllowNoIcons=yes\n"
        "PrivilegesRequired=admin\n"
        "ArchitecturesAllowed=x64compatible\n"
        "ArchitecturesInstallIn64BitMode=x64compatible\n"
        # Explorer caches associations, so the setup asks for a refresh.
        "ChangesAssociations=yes\n"
        "Compression=lzma2\n"
        "SolidCompression=yes\n"
        "WizardStyle=modern\n"
        "UninstallDisplayIcon={app}\\breeze-app.exe\n"
        f"OutputDir={output}\n"
        f"OutputBaseFilename=Breeze-{version}-{target}\n"
        f"SetupIconFile={icon}\n"
        "[Files]\n"
        f'Source: "{payload}\\*"; DestDir: "{{app}}"; Flags: ignoreversion '
        "recursesubdirs createallsubdirs\n"
        "[Icons]\n"
        'Name: "{group}\\Breeze"; Filename: "{app}\\breeze-app.exe"\n'
        "[Registry]\n"
        + "\n".join(registry)
        + "\n"
        "[Run]\n"
        'Filename: "{app}\\breeze-app.exe"; Description: "Launch Breeze"; '
        "Flags: nowait postinstall skipifsilent\n"
    )


def build_payload(version: str, target: str, binary: Path, staging: Path) -> Path:
    """Assemble the files an installer places on the target system.

    Returns the payload root: the `Breeze.app` bundle on macOS, the install
    directory contents on Windows, and the filesystem tree a package manager
    unpacks at `/` on Linux.
    """
    if target not in TARGETS:
        raise ValueError(f"unsupported target: {target!r}")
    if not binary.is_file():
        raise ValueError(f"release binary not found: {binary}")
    match = _version_parts(version)
    core = match["core"]

    if target.endswith("apple-darwin"):
        app = staging / "Breeze.app" / "Contents"
        _copy(binary, app / "MacOS" / EXECUTABLE, executable=True)
        _copy(ROOT / "assets/brand/Breeze.icns", app / "Resources/Breeze.icns")
        resources = app / "Resources"
        _write_documents(resources, version)
        plist = {
            "CFBundleIdentifier": "dev.breeze.reader.prototype",
            "CFBundleName": APP_NAME,
            "CFBundleDisplayName": DISPLAY_NAME,
            "CFBundleExecutable": EXECUTABLE,
            "CFBundleIconFile": "Breeze.icns",
            "CFBundlePackageType": "APPL",
            # A viewer claim puts Breeze in Finder's "Open With" menu.
            # `Alternate` keeps Launch Services from taking a default handler
            # over; changing the default stays a Finder user decision.
            "CFBundleDocumentTypes": [
                {
                    "CFBundleTypeName": "PDF Document",
                    "CFBundleTypeRole": "Viewer",
                    "LSHandlerRank": "Alternate",
                    "LSItemContentTypes": [PDF_CONTENT_TYPE],
                    "CFBundleTypeExtensions": ["pdf"],
                }
            ],
            # Apple's bundle version fields cannot contain SemVer suffixes.
            "CFBundleShortVersionString": core,
            "CFBundleVersion": core,
            "BreezeReleaseVersion": version,
            "LSMinimumSystemVersion": "15.0",
            "NSHighResolutionCapable": True,
        }
        with (app / "Info.plist").open("wb") as handle:
            plistlib.dump(plist, handle)
        return staging / "Breeze.app"

    if target.endswith("windows-msvc"):
        _copy(binary, staging / "breeze-app.exe", executable=True)
        # Copy any adjacent runtime DLLs emitted by the build, if present.
        for dll in binary.parent.glob("*.dll"):
            _copy(dll, staging / dll.name)
        _write_documents(staging, version)
        return staging

    _copy(binary, staging / "usr/bin" / LINUX_EXECUTABLE, executable=True)
    documents = staging / "usr/share/doc" / LINUX_PACKAGE
    _write_documents(documents, version)
    _write_bytes(
        staging / "usr/share/applications" / f"{LINUX_PACKAGE}.desktop",
        desktop_entry().encode("utf-8"),
    )
    _copy(
        ROOT / "assets/brand/breeze-icon-256.png",
        staging / "usr/share/icons/hicolor/256x256/apps" / f"{LINUX_PACKAGE}.png",
    )
    return staging


def _checksum(artifact: Path) -> Path:
    with artifact.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    checksum = artifact.with_name(artifact.name + ".sha256")
    checksum.write_text(f"{digest}  {artifact.name}\n", encoding="utf-8", newline="\n")
    return checksum


def _build_dmg(version: str, target: str, payload: Path, output: Path) -> Path:
    artifact = output / f"Breeze-{version}-{target}.dmg"
    volume = output / ".dmg-volume"
    if volume.exists():
        shutil.rmtree(volume)
    shutil.copytree(payload, volume / payload.name, ignore=METADATA)
    # The usual drag-to-install layout: the bundle beside an Applications link.
    (volume / "Applications").symlink_to("/Applications")
    _run(
        [
            _address_tool("hdiutil"),
            "create",
            "-volname",
            f"{DISPLAY_NAME} {version}",
            "-srcfolder",
            str(volume),
            "-ov",
            "-format",
            "UDZO",
            str(artifact),
        ]
    )
    shutil.rmtree(volume)
    return artifact


def _build_msi(version: str, target: str, payload: Path, output: Path) -> Path:
    directories = WINDOWS_TOOL_DIRS if sys.platform == "win32" else ()
    candle = _address_tool("candle", directories)
    light = _address_tool("light", directories)
    icon = ROOT / "assets/brand/breeze.ico"
    artifact = output / f"Breeze-{version}-{target}.msi"
    work = output / ".wix"
    if work.exists():
        shutil.rmtree(work)
    work.mkdir(parents=True)
    source = work / "breeze.wxs"
    source.write_text(wix_source(version, target, payload, icon), encoding="utf-8")
    _run([candle, "-arch", "x64", "-out", str(work / "breeze.wixobj"), str(source)])
    _run([light, "-out", str(artifact), str(work / "breeze.wixobj")])
    shutil.rmtree(work)
    return artifact


def _build_exe(version: str, target: str, payload: Path, output: Path) -> Path:
    directories = WINDOWS_TOOL_DIRS if sys.platform == "win32" else ()
    iscc = _address_tool("iscc", directories)
    icon = ROOT / "assets/brand/breeze.ico"
    work = output / ".inno"
    work.mkdir(parents=True, exist_ok=True)
    script = work / "breeze.iss"
    script.write_text(
        inno_script(version, target, payload, icon, output), encoding="utf-8"
    )
    _run([iscc, "/Qp", str(script)])
    shutil.rmtree(work)
    return output / f"Breeze-{version}-{target}.exe"


def _build_deb(version: str, target: str, payload: Path, output: Path) -> Path:
    dpkg_deb = _address_tool("dpkg-deb")
    artifact = output / f"Breeze-{version}-{target}.deb"
    work = output / ".deb"
    if work.exists():
        shutil.rmtree(work)
    shutil.copytree(payload, work, ignore=METADATA)
    _write_bytes(
        work / "DEBIAN/control", debian_control(version, payload).encode("utf-8")
    )
    # --root-owner-group keeps the tree reproducible without fakeroot.
    _run([dpkg_deb, "--build", "--root-owner-group", str(work), str(artifact)])
    shutil.rmtree(work)
    return artifact


def _build_appimage(version: str, target: str, payload: Path, output: Path) -> Path:
    tool = _address_tool("appimagetool")
    artifact = output / f"Breeze-{version}-{target}.AppImage"
    appdir = output / f"{DISPLAY_NAME}.AppDir"
    if appdir.exists():
        shutil.rmtree(appdir)
    appdir.mkdir(parents=True)
    shutil.copytree(payload, appdir, dirs_exist_ok=True, ignore=METADATA)
    _copy(ROOT / "assets/brand/breeze-icon-256.png", appdir / f"{LINUX_PACKAGE}.png")
    _write_bytes(
        appdir / f"{LINUX_PACKAGE}.desktop", desktop_entry().encode("utf-8")
    )
    _write_bytes(
        appdir / "AppRun",
        b'#!/bin/sh\nexec "$APPDIR/usr/bin/breeze-app" "$@"\n',
        executable=True,
    )
    environment = {"ARCH": "x86_64"}
    if tool.endswith(".AppImage") or Path(tool).resolve().name.endswith(".AppImage"):
        # appimagetool is itself an AppImage and the runners have no FUSE.
        environment["APPIMAGE_EXTRACT_AND_RUN"] = "1"
    _run([tool, "--no-appstream", str(appdir), str(artifact)], environment)
    shutil.rmtree(appdir)
    return artifact


BUILDERS = {
    "dmg": _build_dmg,
    "msi": _build_msi,
    "exe": _build_exe,
    "deb": _build_deb,
    "appimage": _build_appimage,
}


def _run(command: list[str], environment: dict[str, str] | None = None) -> None:
    merged = dict(os.environ)
    merged.update(environment or {})
    result = subprocess.run(
        command, capture_output=True, text=True, check=False, env=merged
    )
    if result.returncode != 0:
        detail = (result.stderr or result.stdout or "").strip()
        raise ValueError(f"{command[0]} failed ({result.returncode}): {detail}")


def package(version: str, target: str, binary: Path, output: Path) -> tuple[Path, ...]:
    """Build every installer format for one target and write its checksums."""
    _version_parts(version)
    if target not in TARGETS:
        raise ValueError(f"unsupported target: {target!r}")
    if not binary.is_file():
        raise ValueError(f"release binary not found: {binary}")
    output.mkdir(parents=True, exist_ok=True)
    artifacts: list[Path] = []
    with tempfile.TemporaryDirectory(prefix=".payload-", dir=output) as scratch:
        staging = Path(scratch) / "payload"
        staging.mkdir()
        payload = build_payload(version, target, binary, staging)
        for name in FORMATS[target]:
            artifact = BUILDERS[name](version, target, payload, output)
            _checksum(artifact)
            artifacts.append(artifact)
    return tuple(artifacts)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    validate = commands.add_parser("validate", help="skip non-SemVer tags; check Cargo version")
    validate.add_argument("--tag", required=True)
    validate.add_argument("--github-output", type=Path)
    pack = commands.add_parser("package", help="build installers for an already-built binary")
    pack.add_argument("--version", required=True)
    pack.add_argument("--target", choices=TARGETS, required=True)
    pack.add_argument("--binary", type=Path, required=True)
    pack.add_argument("--output", type=Path, default=ROOT / "target/dist")
    args = parser.parse_args()
    try:
        if args.command == "validate":
            metadata = tag_metadata(args.tag, ROOT / "Cargo.toml")
            result = "".join(f"{key}={value}\n" for key, value in metadata.items())
            print(result, end="")
            if args.github_output:
                with args.github_output.open("a", encoding="utf-8") as output:
                    output.write(result)
        else:
            for artifact in package(args.version, args.target, args.binary, args.output):
                print(artifact)
    except (OSError, ValueError, KeyError) as error:
        print(f"release: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Create the one-time r3 bridge by changing only ZIP entry root names.

The canonical release is already built/tested by the central workflow. This
script downloads only a specifically pinned public release, verifies the stable
feed and all three assets, and preserves every app file, permission and plist.
It never installs, executes, renames an installed app, or writes to GitHub.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import io
import json
import os
from pathlib import Path
import plistlib
import re
import stat
import struct
import urllib.parse
import urllib.request
import zipfile

CENTRAL = "Tangram-Space/updates"
LEGACY = "Chevosky/tuner-compact"
FEED_URL = "https://raw.githubusercontent.com/" + CENTRAL + "/main/feeds/turner/stable/release.json"
MAX_ZIP = 256 * 1024 * 1024
MAX_EXPANDED = 512 * 1024 * 1024
VERSION = r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
TAG_PATTERN = re.compile(r"turner-v(" + VERSION + r")-r([1-9][0-9]{0,8})\Z")
SHA_PATTERN = re.compile(r"[0-9a-f]{64}\Z")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


class GitHubRedirects(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, response, code, message, headers, new_url):
        parsed = urllib.parse.urlsplit(new_url)
        require(
            parsed.scheme == "https"
            and parsed.hostname in {
                "github.com", "api.github.com", "raw.githubusercontent.com",
                "objects.githubusercontent.com", "release-assets.githubusercontent.com",
            }
            and parsed.username is None and parsed.password is None,
            "Unexpected GitHub download redirect.",
        )
        return super().redirect_request(request, response, code, message, headers, new_url)


def download(url: str, maximum: int) -> bytes:
    request = urllib.request.Request(url, headers={
        "User-Agent": "Turner-r3-bridge",
        "Accept": "application/vnd.github+json" if url.startswith("https://api.github.com/") else "*/*",
        "Cache-Control": "no-cache",
    })
    opener = urllib.request.build_opener(GitHubRedirects())
    with opener.open(request, timeout=60) as response:
        require(response.status == 200, "The canonical release download was not successful.")
        length = response.headers.get("Content-Length")
        require(length is None or int(length) <= maximum, "Canonical asset exceeds its size limit.")
        data = response.read(maximum + 1)
    require(0 < len(data) <= maximum, "Canonical asset is empty or exceeds its size limit.")
    return data


def validate_marker(marker: dict) -> tuple[str, int]:
    require(isinstance(marker, dict) and set(marker) == {
        "schema_version", "app_id", "from_revision", "canonical_tag", "canonical_manifest_sha256",
    }, "Unexpected bridge marker fields.")
    require(type(marker.get("schema_version")) is int and marker["schema_version"] == 1 and marker.get("app_id") == "turner"
            and type(marker.get("from_revision")) is int and marker["from_revision"] == 3,
            "This marker is only for the one-time Tuner Compact r3 migration.")
    require(isinstance(marker.get("canonical_tag"), str), "Missing canonical tag.")
    match = TAG_PATTERN.fullmatch(marker["canonical_tag"])
    require(match is not None and int(match[2]) > 3, "The canonical revision must be newer than r3.")
    require(isinstance(marker.get("canonical_manifest_sha256"), str)
            and SHA_PATTERN.fullmatch(marker["canonical_manifest_sha256"]) is not None,
            "Pin the exact SHA-256 of the canonical turner-update.json.")
    return match[1], int(match[2])


def fetch_canonical(marker: dict, fetch=download) -> tuple[dict, bytes]:
    version, revision = validate_marker(marker)
    tag = marker["canonical_tag"]
    zip_name = f"Turner-v{version}-r{revision}-macos-x86_64.app.zip"
    names = [zip_name, zip_name + ".sha256", "turner-update.json"]
    release = json.loads(fetch(f"https://api.github.com/repos/{CENTRAL}/releases/tags/{tag}", 1024 * 1024))
    require(isinstance(release, dict) and release.get("tag_name") == tag
            and release.get("draft") is False and release.get("prerelease") is False,
            "The canonical release is not a complete stable release.")
    assets = release.get("assets", [])
    require(isinstance(assets, list) and len(assets) == 3
            and {asset.get("name") for asset in assets} == set(names),
            "The canonical release must contain exactly its three update assets.")
    records = {asset["name"]: asset for asset in assets}
    base = f"https://github.com/{CENTRAL}/releases/download/{tag}/"

    def asset_bytes(name: str, maximum: int) -> bytes:
        record = records[name]
        require(record.get("state") == "uploaded" and type(record.get("size")) is int
                and 0 < record["size"] <= maximum
                and record.get("browser_download_url") == base + name,
                "Unexpected canonical asset metadata: " + name)
        value = fetch(base + name, maximum)
        require(len(value) == record["size"] and record.get("digest") == "sha256:" + digest(value),
                "Canonical asset size or GitHub digest mismatch: " + name)
        return value

    manifest_bytes = asset_bytes("turner-update.json", 16384)
    require(digest(manifest_bytes) == marker["canonical_manifest_sha256"],
            "The canonical manifest does not match the explicit bridge marker.")
    # A release becomes the channel only after its feed commit succeeds.
    feed_bytes = fetch(FEED_URL, 16384)
    require(feed_bytes == manifest_bytes, "The pinned release is not the current Turner stable feed.")
    manifest = json.loads(manifest_bytes)
    expected = {
        "schema_version": 1, "app_id": "turner", "channel": "stable",
        "version": version, "revision": revision, "tag": tag, "platform": "macos",
        "arch": "x86_64", "minimum_macos": "11.0", "bundle_id": "com.tuner.guitar.compact",
        "executable": "guitar-tuner-egui", "asset": zip_name, "url": base + zip_name,
    }
    require(isinstance(manifest, dict) and all(manifest.get(key) == value for key, value in expected.items())
            and type(manifest.get("revision")) is int and type(manifest.get("schema_version")) is int
            and isinstance(manifest.get("sha256"), str) and SHA_PATTERN.fullmatch(manifest["sha256"]) is not None,
            "Invalid canonical Turner update manifest.")
    checksum = asset_bytes(zip_name + ".sha256", 1024)
    require(checksum == (manifest["sha256"] + "  " + zip_name + "\n").encode(),
            "The canonical checksum does not agree with its manifest.")
    canonical_zip = asset_bytes(zip_name, MAX_ZIP)
    require(digest(canonical_zip) == manifest["sha256"], "The canonical ZIP hash does not agree.")
    return manifest, canonical_zip


def legacy_name(name: str) -> str:
    require("\x00" not in name and "\\" not in name and not name.startswith("/"),
            "Unsafe ZIP path.")
    parts = name.rstrip("/").split("/")
    require(parts and all(part not in {"", ".", ".."} for part in parts), "Unsafe ZIP path components.")
    if name == "Turner.app" or name.startswith("Turner.app/"):
        return "Tuner Compact.app" + name[len("Turner.app"):]
    if name == "__MACOSX/":
        return name
    if name == "__MACOSX/._Turner.app":
        return "__MACOSX/._Tuner Compact.app"
    prefix = "__MACOSX/Turner.app"
    if name == prefix or name.startswith(prefix + "/"):
        return "__MACOSX/Tuner Compact.app" + name[len(prefix):]
    raise ValueError("Unexpected canonical ZIP root: " + name)


def validate_macho(data: bytes) -> None:
    require(len(data) >= 32, "Truncated executable.")
    magic, cpu, _subtype, file_type, count, commands_size, _flags, _reserved = struct.unpack_from("<8I", data)
    require(magic == 0xFEEDFACF and cpu == 0x01000007 and file_type == 2,
            "The canonical executable must be a thin Intel x86_64 Mach-O.")
    require(0 < count <= 8192 and 0 < commands_size <= len(data) - 32, "Invalid Mach-O commands.")
    cursor, end, minimums = 32, 32 + commands_size, []
    for _ in range(count):
        require(cursor + 8 <= end, "Truncated Mach-O load command.")
        command, size = struct.unpack_from("<II", data, cursor)
        require(size >= 8 and size % 8 == 0 and cursor + size <= end, "Invalid Mach-O command size.")
        if command == 0x32:
            require(size >= 24, "Truncated LC_BUILD_VERSION.")
            platform, minimum = struct.unpack_from("<II", data, cursor + 8)
            require(platform == 1, "The executable is not for macOS.")
            minimums.append(minimum)
        elif command == 0x24:
            require(size >= 16, "Truncated LC_VERSION_MIN_MACOSX.")
            minimums.append(struct.unpack_from("<I", data, cursor + 8)[0])
        cursor += size
    require(cursor == end and minimums and all(value <= 0x000B0000 for value in minimums),
            "The executable requires a system newer than macOS 11.0.")


def repack_archive(canonical_zip: bytes, version: str, revision: int,
                   expected_resources: dict[str, bytes]) -> bytes:
    require(0 < len(canonical_zip) <= MAX_ZIP, "Invalid ZIP size.")
    with zipfile.ZipFile(io.BytesIO(canonical_zip)) as source:
        entries = source.infolist()
        require(0 < len(entries) <= 8192 and sum(info.file_size for info in entries) <= MAX_EXPANDED,
                "The canonical archive is too large.")
        names = [info.filename for info in entries]
        require(len(set(names)) == len(names), "Duplicate canonical archive entry.")
        mapped = [legacy_name(name) for name in names]
        require(len(set(mapped)) == len(mapped), "Duplicate legacy archive entry.")
        for info in entries:
            mode = info.external_attr >> 16
            kind = stat.S_IFMT(mode)
            require(not info.flag_bits & 1 and info.compress_type in {zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED},
                    "Unsupported encrypted/compressed archive entry.")
            require(kind in {0, stat.S_IFDIR if info.is_dir() else stat.S_IFREG}, "Links or special files are not permitted.")
            cursor = 0
            while cursor < len(info.extra):
                require(cursor + 4 <= len(info.extra), "Truncated ZIP extra field.")
                field, size = struct.unpack_from("<HH", info.extra, cursor)
                require(field != 1 and cursor + 4 + size <= len(info.extra), "ZIP64 or invalid ZIP extra field.")
                cursor += 4 + size
        require(source.testzip() is None, "A canonical ZIP entry failed its CRC.")
        plist = plistlib.loads(source.read("Turner.app/Contents/Info.plist"))
        expected_plist = {
            "CFBundleIdentifier": "com.tuner.guitar.compact", "CFBundleExecutable": "guitar-tuner-egui",
            "CFBundleName": "Turner", "CFBundleDisplayName": "Turner",
            "CFBundleShortVersionString": version, "CFBundleVersion": str(revision),
            "LSMinimumSystemVersion": "11.0",
        }
        require(all(plist.get(key) == value for key, value in expected_plist.items())
                and isinstance(plist.get("NSMicrophoneUsageDescription"), str)
                and bool(plist["NSMicrophoneUsageDescription"].strip()), "Unexpected canonical bundle identity.")
        executable = source.getinfo("Turner.app/Contents/MacOS/guitar-tuner-egui")
        require(executable.external_attr >> 16 & 0o111 != 0, "The executable lacks its execute permission.")
        validate_macho(source.read(executable))
        for name, expected in expected_resources.items():
            require(source.read("Turner.app/Contents/Resources/" + name) == expected,
                    "The application icon or license changed: " + name)
        output = io.BytesIO()
        with zipfile.ZipFile(output, "w", allowZip64=False) as destination:
            destination.comment = source.comment
            for original, name in zip(entries, mapped):
                copied = copy.copy(original)
                copied.filename = copied.orig_filename = name
                destination.writestr(copied, source.read(original))
        legacy_zip = output.getvalue()
        require(len(legacy_zip) <= MAX_ZIP, "The legacy ZIP exceeds the updater's download limit.")
        # The repack changes names, never app content or filesystem metadata.
        with zipfile.ZipFile(io.BytesIO(legacy_zip)) as destination:
            require(destination.testzip() is None, "The legacy ZIP failed its CRC.")
            for original, name in zip(entries, mapped):
                copied = destination.getinfo(name)
                require(source.read(original) == destination.read(copied), "A file changed during repack.")
                for field in ("date_time", "compress_type", "create_system", "external_attr",
                              "internal_attr", "extra", "comment"):
                    require(getattr(original, field) == getattr(copied, field), "ZIP metadata changed: " + field)
    return legacy_zip


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("marker", type=Path)
    parser.add_argument("output", type=Path)
    arguments = parser.parse_args()
    marker = json.loads(arguments.marker.read_bytes())
    manifest, canonical_zip = fetch_canonical(marker)
    root = Path(__file__).resolve().parent.parent
    expected_resources = {
        "icon.icns": (root / "assets/icons/icon.icns").read_bytes(),
        "Licenses/Tuner-MIT.txt": (root / "LICENSE").read_bytes(),
        "Licenses/CPAL-Apache-2.0.txt": (root / "vendor/cpal/LICENSE").read_bytes(),
    }
    version, revision = manifest["version"], manifest["revision"]
    legacy_zip = repack_archive(canonical_zip, version, revision, expected_resources)
    tag = f"compact-v{version}-r{revision}"
    zip_name = f"Tuner-Compact-v{version}-r{revision}-macos-x86_64.app.zip"
    checksum_name, manifest_name = zip_name + ".sha256", "tuner-compact-update.json"
    legacy_manifest = {key: value for key, value in manifest.items() if key not in {"app_id", "channel"}}
    legacy_manifest.update(tag=tag, asset=zip_name,
                           url=f"https://github.com/{LEGACY}/releases/download/{tag}/{zip_name}",
                           sha256=digest(legacy_zip))
    arguments.output.mkdir(parents=True, exist_ok=False)
    (arguments.output / zip_name).write_bytes(legacy_zip)
    (arguments.output / checksum_name).write_text(digest(legacy_zip) + "  " + zip_name + "\n", encoding="utf-8")
    (arguments.output / manifest_name).write_text(json.dumps(legacy_manifest, indent=2) + "\n", encoding="utf-8")
    output_values = {
        "version": version, "revision": revision, "tag": tag, "zip_name": zip_name,
        "checksum_name": checksum_name, "manifest_name": manifest_name,
        "artifact_directory": str(arguments.output.resolve()),
        "canonical_tag": marker["canonical_tag"],
        "canonical_manifest_sha256": marker["canonical_manifest_sha256"],
        "legacy_manifest_sha256": digest((arguments.output / manifest_name).read_bytes()),
    }
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
            for key, value in output_values.items():
                print(f"{key}={value}", file=output)
    print(json.dumps(output_values, indent=2))


if __name__ == "__main__":
    main()

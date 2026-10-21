#!/usr/bin/env python3
"""Offline tests of the actual bridge downloader and metadata-preserving repack."""
import copy
import importlib.util
import io
import json
from pathlib import Path
import plistlib
import stat
import struct
import unittest
import zipfile

script = Path(__file__).with_name("repack-turner-r3-bridge.py")
spec = importlib.util.spec_from_file_location("turner_r3_bridge", script)
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)

VERSION, REVISION = "0.1.13", 4
TAG = f"turner-v{VERSION}-r{REVISION}"
ZIP_NAME = f"Turner-v{VERSION}-r{REVISION}-macos-x86_64.app.zip"
RESOURCES = {
    "icon.icns": b"unchanged ICNS fixture",
    "Licenses/Tuner-MIT.txt": b"unchanged MIT license fixture",
    "Licenses/CPAL-Apache-2.0.txt": b"unchanged Apache license fixture",
}


def archive_fixture(extra_name=None, symlink=False, minimum=0x000B0000, revision=REVISION):
    binary = struct.pack("<8I", 0xFEEDFACF, 0x01000007, 3, 2, 1, 16, 0, 0)
    binary += struct.pack("<4I", 0x24, 16, minimum, 0x000F0000)
    plist = plistlib.dumps({
        "CFBundleIdentifier": "com.tuner.guitar.compact", "CFBundleExecutable": "guitar-tuner-egui",
        "CFBundleName": "Turner", "CFBundleDisplayName": "Turner",
        "CFBundleShortVersionString": VERSION, "CFBundleVersion": str(revision),
        "LSMinimumSystemVersion": "11.0", "NSMicrophoneUsageDescription": "Afinar con el micrófono.",
    })
    files = {
        "Turner.app/": b"",
        "Turner.app/Contents/": b"",
        "Turner.app/Contents/Info.plist": plist,
        "Turner.app/Contents/MacOS/guitar-tuner-egui": binary,
        **{"Turner.app/Contents/Resources/" + name: value for name, value in RESOURCES.items()},
        "__MACOSX/": b"",
        "__MACOSX/._Turner.app": b"AppleDouble fixture",
        "__MACOSX/Turner.app/Contents/._Info.plist": b"file metadata fixture",
    }
    if extra_name:
        files[extra_name] = b"unexpected"
    output = io.BytesIO()
    with zipfile.ZipFile(output, "w") as archive:
        archive.comment = b"preserved archive comment"
        for name, value in files.items():
            item = zipfile.ZipInfo(name, (2026, 10, 21, 12, 34, 56))
            item.create_system = 3
            executable = name.endswith("/guitar-tuner-egui")
            mode = (stat.S_IFDIR | 0o755) if name.endswith("/") else (stat.S_IFREG | (0o755 if executable else 0o644))
            if symlink and executable:
                mode = stat.S_IFLNK | 0o777
            item.external_attr = mode << 16
            item.compress_type = zipfile.ZIP_DEFLATED
            item.extra = struct.pack("<HH", 0x5855, 8) + b"\0" * 8
            item.comment = b"preserved entry comment"
            archive.writestr(item, value)
    return output.getvalue()


def download_fixture():
    canonical_zip = archive_fixture()
    base = f"https://github.com/{bridge.CENTRAL}/releases/download/{TAG}/"
    manifest = {
        "schema_version": 1, "app_id": "turner", "channel": "stable",
        "version": VERSION, "revision": REVISION, "tag": TAG, "platform": "macos", "arch": "x86_64",
        "minimum_macos": "11.0", "bundle_id": "com.tuner.guitar.compact", "executable": "guitar-tuner-egui",
        "asset": ZIP_NAME, "url": base + ZIP_NAME, "sha256": bridge.digest(canonical_zip),
    }
    manifest_bytes = (json.dumps(manifest, indent=2) + "\n").encode()
    files = {
        ZIP_NAME: canonical_zip,
        ZIP_NAME + ".sha256": (manifest["sha256"] + "  " + ZIP_NAME + "\n").encode(),
        "turner-update.json": manifest_bytes,
    }
    marker = {
        "schema_version": 1, "app_id": "turner", "from_revision": 3,
        "canonical_tag": TAG, "canonical_manifest_sha256": bridge.digest(manifest_bytes),
    }
    release = {
        "tag_name": TAG, "draft": False, "prerelease": False,
        "assets": [{
            "name": name, "state": "uploaded", "size": len(data),
            "digest": "sha256:" + bridge.digest(data), "browser_download_url": base + name,
        } for name, data in files.items()],
    }
    payloads = {base + name: data for name, data in files.items()}
    release_url = f"https://api.github.com/repos/{bridge.CENTRAL}/releases/tags/{TAG}"
    payloads[release_url] = json.dumps(release).encode()
    payloads[bridge.FEED_URL] = manifest_bytes
    calls = []

    def fetch(url, maximum):
        calls.append(url)
        assert "/latest" not in url
        value = payloads[url]
        assert 0 < len(value) <= maximum
        return value

    return marker, manifest, payloads, calls, fetch, base, release_url, release


class BridgeTests(unittest.TestCase):
    def test_repack_changes_only_root_names_and_keeps_content_icon_permissions_licenses(self):
        canonical = archive_fixture()
        repacked = bridge.repack_archive(canonical, VERSION, REVISION, RESOURCES)
        self.assertNotEqual(canonical, repacked)
        with zipfile.ZipFile(io.BytesIO(canonical)) as original, zipfile.ZipFile(io.BytesIO(repacked)) as legacy:
            self.assertIsNone(legacy.testzip())
            self.assertEqual(original.comment, legacy.comment)
            for entry in original.infolist():
                transformed = legacy.getinfo(bridge.legacy_name(entry.filename))
                self.assertEqual(original.read(entry), legacy.read(transformed))
                for field in ("date_time", "compress_type", "create_system", "external_attr", "internal_attr", "extra", "comment"):
                    self.assertEqual(getattr(entry, field), getattr(transformed, field), field)
            executable = legacy.getinfo("Tuner Compact.app/Contents/MacOS/guitar-tuner-egui")
            self.assertEqual(executable.external_attr >> 16, stat.S_IFREG | 0o755)
            plist = plistlib.loads(legacy.read("Tuner Compact.app/Contents/Info.plist"))
            self.assertEqual(plist["CFBundleDisplayName"], "Turner")
            self.assertEqual(plist["CFBundleVersion"], "4")

    def test_canonical_release_feed_marker_and_assets_must_all_agree(self):
        marker, manifest, _, calls, fetch, _, _, _ = download_fixture()
        actual, package = bridge.fetch_canonical(marker, fetch)
        self.assertEqual(actual, manifest)
        self.assertEqual(bridge.digest(package), manifest["sha256"])
        self.assertEqual(len(calls), 5)

    def test_a_different_pin_or_stale_feed_is_rejected(self):
        marker, _, payloads, _, fetch, _, _, _ = download_fixture()
        changed = dict(marker, canonical_manifest_sha256="0" * 64)
        with self.assertRaisesRegex(ValueError, "explicit bridge marker"):
            bridge.fetch_canonical(changed, fetch)
        payloads[bridge.FEED_URL] += b"\n"
        with self.assertRaisesRegex(ValueError, "current Turner stable feed"):
            bridge.fetch_canonical(marker, fetch)

    def test_download_tampering_is_rejected(self):
        for asset in (ZIP_NAME, ZIP_NAME + ".sha256", "turner-update.json"):
            with self.subTest(asset=asset):
                marker, _, payloads, _, fetch, base, _, _ = download_fixture()
                payloads[base + asset] += b"changed"
                with self.assertRaisesRegex(ValueError, "size or GitHub digest"):
                    bridge.fetch_canonical(marker, fetch)

    def test_incomplete_or_nonstable_release_is_rejected(self):
        for field, value in (("draft", True), ("prerelease", True), ("tag_name", "airplay-v1")):
            with self.subTest(field=field):
                marker, _, payloads, _, fetch, _, url, release = download_fixture()
                release[field] = value
                payloads[url] = json.dumps(release).encode()
                with self.assertRaisesRegex(ValueError, "complete stable release"):
                    bridge.fetch_canonical(marker, fetch)
        marker, _, payloads, _, fetch, _, url, release = download_fixture()
        release["assets"].pop()
        payloads[url] = json.dumps(release).encode()
        with self.assertRaisesRegex(ValueError, "exactly its three"):
            bridge.fetch_canonical(marker, fetch)

    def test_marker_cannot_target_another_app_unpinned_release_or_old_revision(self):
        marker, *_ = download_fixture()
        for fields in (
            {"canonical_manifest_sha256": "not-a-hash"}, {"canonical_tag": "turner-v0.1.13-r3"},
            {"canonical_tag": "airplay-v1.0.0-r4"}, {"from_revision": 2}, {"schema_version": True},
            {"unexpected": "field"},
        ):
            with self.subTest(fields=fields), self.assertRaises(ValueError):
                bridge.validate_marker(dict(marker, **fields))

    def test_unsafe_roots_and_links_are_rejected(self):
        for name in ("../outside", "Turner.app/../outside", "/Turner.app/file", "Other.app/file", "Turner.app\\file"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                bridge.repack_archive(archive_fixture(extra_name=name), VERSION, REVISION, RESOURCES)
        with self.assertRaisesRegex(ValueError, "Links or special"):
            bridge.repack_archive(archive_fixture(symlink=True), VERSION, REVISION, RESOURCES)

    def test_wrong_bundle_revision_icon_or_minimum_macos_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "bundle identity"):
            bridge.repack_archive(archive_fixture(revision=5), VERSION, REVISION, RESOURCES)
        with self.assertRaisesRegex(ValueError, "newer than macOS 11.0"):
            bridge.repack_archive(archive_fixture(minimum=0x000C0000), VERSION, REVISION, RESOURCES)
        changed = dict(RESOURCES, **{"icon.icns": b"changed"})
        with self.assertRaisesRegex(ValueError, "icon or license changed"):
            bridge.repack_archive(archive_fixture(), VERSION, REVISION, changed)


if __name__ == "__main__":
    unittest.main()

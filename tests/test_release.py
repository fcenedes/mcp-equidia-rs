import io
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

from scripts import release


class ReleaseChecks(unittest.TestCase):
    def test_tag_must_match_package_version(self):
        self.assertEqual(release.verify_tag("v" + release.VERSION), release.VERSION)
        for value in ["v999.0.0", release.VERSION, "v0.1.0\nmalicious"]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                release.verify_tag(value)

    def test_registry_names_are_normalized_and_validated(self):
        metadata = release.image_metadata("Owner/Repo", "v" + release.VERSION)
        self.assertEqual(metadata["image"], "ghcr.io/owner/repo")
        self.assertIn("ghcr.io/owner/repo:" + release.VERSION, metadata["tags"])
        with self.assertRaises(ValueError):
            release.image_metadata("owner/repo\ninjected", "v" + release.VERSION)

    def test_archive_tampering_is_detected(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            archive = output / release.archive_name(release.TARGETS[0])
            archive.write_bytes(b"original payload")
            release.checksum(archive)
            release.checksums(output, require_all=False)
            self.assertIn(archive.name, (output / "SHA256SUMS").read_text())
            archive.write_bytes(b"modified payload")
            with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                release.checksums(output, require_all=False)

    def test_release_requires_all_four_platforms(self):
        with tempfile.TemporaryDirectory() as temporary, self.assertRaises(ValueError):
            release.checksums(Path(temporary), require_all=True)

    def test_package_audit_rejects_credentials_traversal_and_links(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "bad.crate"
            for name, linked in [("api/.env", False), ("../escape", False), ("src/link", True)]:
                with self.subTest(name=name):
                    with tarfile.open(path, "w:gz") as archive:
                        member = tarfile.TarInfo(f"{release.NAME}-{release.VERSION}/{name}")
                        if linked:
                            member.type = tarfile.SYMTYPE
                            member.linkname = "/outside"
                            archive.addfile(member)
                        else:
                            data = b"secret"
                            member.size = len(data)
                            archive.addfile(member, io.BytesIO(data))
                    with self.assertRaises(ValueError):
                        release.audit_package(path)

    def test_invalid_image_digest_cannot_reach_docker(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary)
            (path / "digest-amd64.txt").write_text("foreign.example/other:latest")
            with patch.object(release.subprocess, "run") as command, self.assertRaises(ValueError):
                release.image_manifest("owner/repo", "v" + release.VERSION, path)
            command.assert_not_called()


if __name__ == "__main__":
    unittest.main()

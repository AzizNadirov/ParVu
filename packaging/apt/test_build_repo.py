"""Tests for build_repo.py: python3 -m unittest discover -s packaging/apt"""

import gzip
import subprocess
import unittest

import build_repo as br


def rel(tag, prerelease=False, draft=False, date="2026-01-01T00:00:00Z", deb=True):
    assets = [{"name": "SHA256SUMS.txt"}, {"name": f"ParVu_{tag[1:]}_x64-setup.exe"}]
    if deb:
        assets.append({"name": f"parvu_{tag[1:]}_amd64.deb"})
    return {"tag_name": tag, "prerelease": prerelease, "draft": draft, "published_at": date, "assets": assets}


class Versions(unittest.TestCase):
    def test_matches_dpkg(self):
        pairs = [("1.0.0~alpha.11", "1.0.0"), ("1.0.0~alpha.2", "1.0.0~alpha.11"),
                 ("1.0.0", "1.0.1"), ("1.0.9", "1.0.10"), ("1.1.0~beta.1", "1.1.0~rc.1"),
                 ("1.0.0", "1.0.0"), ("1.2.0", "1.10.0"), ("1:0.1", "2.0")]
        for a, b in pairs:
            for x, y in ((a, b), (b, a)):
                ours = br.compare_versions(x, y)
                dpkg = subprocess.run(["dpkg", "--compare-versions", x, "lt", y]).returncode == 0
                self.assertEqual(ours < 0, dpkg, (x, y))

    def test_tag_to_version(self):
        self.assertEqual(br.tag_to_version("v1.0.0-alpha.11"), "1.0.0~alpha.11")
        self.assertEqual(br.tag_to_version("v1.0.0"), "1.0.0")
        for tag in ("latest", "beta", "v2b", "v003b", "1.0.0", "v1.0"):
            self.assertIsNone(br.tag_to_version(tag))


class Plan(unittest.TestCase):
    def test_before_stable_only_floor_matters(self):
        plan = br.make_plan([rel("v1.0.0-alpha.11"), rel("v0.3.1"), rel("latest")])
        self.assertEqual(plan["suites"], {"stable": [], "beta": []})

    def test_stable_and_beta(self):
        releases = [
            rel("v1.0.0-alpha.11"), rel("v1.0.0", date="2026-02-01T00:00:00Z"),
            rel("v1.0.1"), rel("v1.0.2"), rel("v1.0.3", date="2026-03-01T00:00:00Z"),
            rel("v1.1.0-beta.1", prerelease=True, date="2026-04-01T00:00:00Z"),
            rel("v1.1.0-beta.2", prerelease=True, draft=True),
            rel("v1.0.4", prerelease=True),  # marked by hand: beta only
            rel("v1.2.0", deb=False),
        ]
        plan = br.make_plan(releases)
        self.assertEqual(plan["suites"]["stable"], ["v1.0.3", "v1.0.2", "v1.0.1"])
        self.assertEqual(plan["suites"]["beta"], ["v1.1.0-beta.1", "v1.0.4", "v1.0.3"])
        self.assertEqual(plan["date"], "2026-04-01T00:00:00Z")
        self.assertEqual([r["tag"] for r in plan["releases"]],
                         ["v1.1.0-beta.1", "v1.0.4", "v1.0.3", "v1.0.2", "v1.0.1"])

    def test_beta_drops_prereleases_older_than_stable(self):
        plan = br.make_plan([rel("v1.1.0-beta.1", prerelease=True), rel("v1.1.0"), rel("v1.0.0")])
        self.assertEqual(plan["suites"]["beta"], ["v1.1.0"])

    def test_treat_as_prerelease_and_ndjson(self):
        text = "\n".join(__import__("json").dumps(r) for r in [rel("v1.0.0-alpha.11"), rel("v1.0.0-alpha.10")])
        plan = br.make_plan(br.load_releases(text), min_version="1.0.0~alpha.10",
                            treat_as_prerelease=("v1.0.0-alpha.11",))
        self.assertEqual(plan["suites"], {"stable": [], "beta": ["v1.0.0-alpha.11", "v1.0.0-alpha.10"]})


class Files(unittest.TestCase):
    def test_gzip_is_deterministic(self):
        self.assertEqual(br.gzip_bytes(b"abc"), br.gzip_bytes(b"abc"))
        self.assertEqual(gzip.decompress(br.gzip_bytes(b"abc")), b"abc")

    def test_release_file(self):
        text = br.release_file("stable", "Mon, 23 Nov 2026 10:00:00 UTC", {"main/binary-amd64/Packages": b""})
        self.assertIn("Codename: stable\n", text)
        self.assertNotIn("Valid-Until", text)
        self.assertIn(" e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855                0 "
                      "main/binary-amd64/Packages\n", text)

    def test_control_parsing_keeps_continuations(self):
        fields = br.parse_control("Package: parvu\nDescription: short\n long line\n .\n more\n")
        self.assertEqual(fields[1], ("Description", "short\n long line\n .\n more"))


if __name__ == "__main__":
    unittest.main()

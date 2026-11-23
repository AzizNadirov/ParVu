#!/usr/bin/env python3
"""Build ParVu's signed APT repository (published on GitHub Pages).

Two steps, both deterministic:

  build_repo.py plan --releases releases.json > plan.json
      Chooses which GitHub releases go into which suite. releases.json is the
      GitHub API's release list (a JSON array, or one release object per line,
      as `gh api --paginate .../releases --jq '.[]'` prints it).

        stable  releases that are not pre-releases and whose tag has no "-"
        beta    the newest stable release plus every newer release
                (pre-releases and "-" tags), so beta users also get stable
                updates

      Each suite keeps the newest --keep versions (default 3). Versions below
      --min-version (default 1.0.0) are left out: the 1.0.0 alphas and the
      0.x Python app predate the repository. Tags that aren't vX.Y.Z or
      vX.Y.Z-something, drafts, and releases without a parvu_*_amd64.deb are
      ignored.

  build_repo.py build --plan plan.json --debs DIR --out SITE \\
        --public-key parvu-archive-keyring.asc --fingerprint FPR \\
        [--gnupghome DIR] [--unsigned]
      Writes the site: SITE/index.html and SITE/apt/ with the pool, the
      Packages indices, Release, InRelease and Release.gpg for each suite,
      and the public keyring. The debs are read from DIR/<tag>/<asset>, as
      the workflow downloads them. Signing uses the secret key with
      fingerprint FPR from GNUPGHOME (or $GNUPGHOME); the public key file must
      hold exactly that key, so the repository can't be signed with a key
      users don't have.

Only the Python standard library, dpkg-deb and gpg are needed. The same plan
and debs always give the same files, apart from the signatures (gpg puts the
signing time in them). Release's Date is the newest included release's
publication time, so it only moves forward when a release is added.
"""

from __future__ import annotations

import argparse
import datetime as dt
import functools
import gzip
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ORIGIN = "ParVu"
LABEL = "ParVu"
COMPONENT = "main"
ARCH = "amd64"
PACKAGE = "parvu"
SUITES = ("stable", "beta")
SITE_URL = "https://aziznadirov.github.io/ParVu"
REPO_URL = "https://github.com/AzizNadirov/ParVu"
TEMPLATES = Path(__file__).resolve().parent / "site"

TAG_RE = re.compile(r"^v(\d+\.\d+\.\d+)(?:-([0-9A-Za-z][0-9A-Za-z.]*))?$")
ASSET_RE = re.compile(rf"^{PACKAGE}_[^_/]+_{ARCH}\.deb$")


class RepoError(Exception):
    pass


# ---------------------------------------------------------------------------
# Debian version comparison (as dpkg --compare-versions), for sorting
# ---------------------------------------------------------------------------

def _order(c: str) -> int:
    if c == "~":
        return -1
    if c.isdigit():
        return 0
    if c.isalpha():
        return ord(c)
    return ord(c) + 256


def _compare_part(a: str, b: str) -> int:
    i = j = 0
    while i < len(a) or j < len(b):
        first_diff = 0
        while (i < len(a) and not a[i].isdigit()) or (j < len(b) and not b[j].isdigit()):
            ac = _order(a[i]) if i < len(a) else 0
            bc = _order(b[j]) if j < len(b) else 0
            if ac != bc:
                return ac - bc
            i += 1
            j += 1
        while i < len(a) and a[i] == "0":
            i += 1
        while j < len(b) and b[j] == "0":
            j += 1
        while i < len(a) and a[i].isdigit() and j < len(b) and b[j].isdigit():
            if not first_diff:
                first_diff = ord(a[i]) - ord(b[j])
            i += 1
            j += 1
        if i < len(a) and a[i].isdigit():
            return 1
        if j < len(b) and b[j].isdigit():
            return -1
        if first_diff:
            return first_diff
    return 0


def _split_version(v: str) -> tuple[int, str, str]:
    epoch = 0
    if ":" in v:
        e, v = v.split(":", 1)
        epoch = int(e)
    upstream, _, revision = v.rpartition("-") if "-" in v else (v, "", "")
    return epoch, upstream, revision


def compare_versions(a: str, b: str) -> int:
    """<0, 0 or >0 as Debian version a sorts before, equal to or after b."""
    ea, ua, ra = _split_version(a)
    eb, ub, rb = _split_version(b)
    if ea != eb:
        return ea - eb
    return _compare_part(ua, ub) or _compare_part(ra, rb)


_version_key = functools.cmp_to_key(compare_versions)


def tag_to_version(tag: str) -> str | None:
    """v1.0.0-alpha.11 -> 1.0.0~alpha.11 (the deb's control Version)."""
    m = TAG_RE.match(tag)
    if not m:
        return None
    return m.group(1) + (f"~{m.group(2)}" if m.group(2) else "")


# ---------------------------------------------------------------------------
# plan
# ---------------------------------------------------------------------------

def load_releases(text: str) -> list[dict]:
    text = text.strip()
    if not text:
        return []
    try:
        data = json.loads(text)
    except json.JSONDecodeError:
        data = [json.loads(line) for line in text.splitlines() if line.strip()]
    if isinstance(data, dict):
        data = [data]
    flat: list[dict] = []
    for item in data:  # `gh api --paginate` without --jq gives one array per page
        flat.extend(item if isinstance(item, list) else [item])
    return flat


def make_plan(releases: list[dict], keep: int = 3, min_version: str = "1.0.0",
              treat_as_prerelease: tuple[str, ...] = ()) -> dict:
    candidates = []
    for rel in releases:
        tag = rel.get("tag_name") or rel.get("tagName") or ""
        version = tag_to_version(tag)
        if version is None or rel.get("draft") or rel.get("isDraft"):
            continue
        if compare_versions(version, min_version) < 0:
            continue
        debs = sorted(a["name"] for a in rel.get("assets", []) if ASSET_RE.match(a.get("name", "")))
        if not debs:
            continue
        prerelease = bool(rel.get("prerelease") or rel.get("isPrerelease")) or tag in treat_as_prerelease
        candidates.append({
            "tag": tag,
            "version": version,
            "asset": debs[0],
            "published_at": rel.get("published_at") or rel.get("publishedAt") or "",
            "stable": not prerelease and "-" not in tag,
        })
    newest_first = sorted(candidates, key=lambda c: _version_key(c["version"]), reverse=True)
    stable = [c for c in newest_first if c["stable"]]
    if stable:
        beta = [c for c in newest_first if compare_versions(c["version"], stable[0]["version"]) >= 0]
    else:
        beta = newest_first
    suites = {"stable": stable[:keep], "beta": beta[:keep]}
    entries = {c["tag"]: c for s in suites.values() for c in s}
    dates = [c["published_at"] for c in entries.values() if c["published_at"]]
    return {
        "date": max(dates) if dates else "",
        "releases": [{k: v for k, v in c.items() if k != "stable"}
                     for c in sorted(entries.values(), key=lambda c: _version_key(c["version"]), reverse=True)],
        "suites": {name: [c["tag"] for c in s] for name, s in suites.items()},
    }


# ---------------------------------------------------------------------------
# build
# ---------------------------------------------------------------------------

def run(cmd: list[str], **kw) -> subprocess.CompletedProcess:
    try:
        return subprocess.run(cmd, check=True, capture_output=True, **kw)
    except subprocess.CalledProcessError as e:
        err = (e.stderr or b"").decode(errors="replace").strip()
        raise RepoError(f"{' '.join(cmd[:3])}… failed: {err}") from None


def parse_control(text: str) -> list[tuple[str, str]]:
    """Fields of a control paragraph, in order; values keep continuation lines."""
    fields: list[tuple[str, str]] = []
    for line in text.splitlines():
        if not line.strip():
            continue
        if line[0] in " \t":
            if not fields:
                raise RepoError("malformed control file")
            name, value = fields[-1]
            fields[-1] = (name, value + "\n" + line)
        else:
            name, _, value = line.partition(":")
            fields.append((name.strip(), value.strip()))
    return fields


def file_hashes(path: Path) -> dict[str, str]:
    md5, sha1, sha256 = hashlib.md5(), hashlib.sha1(), hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            md5.update(chunk)
            sha1.update(chunk)
            sha256.update(chunk)
    return {"MD5sum": md5.hexdigest(), "SHA1": sha1.hexdigest(), "SHA256": sha256.hexdigest()}


def package_stanza(deb: Path, filename: str, expect_version: str) -> tuple[str, str]:
    control = run(["dpkg-deb", "-f", str(deb)]).stdout.decode()
    fields = parse_control(control)
    info = dict(fields)
    if info.get("Package") != PACKAGE or info.get("Architecture") != ARCH:
        raise RepoError(f"{deb.name}: expected package {PACKAGE} for {ARCH}, "
                        f"found {info.get('Package')} for {info.get('Architecture')}")
    if info.get("Version") != expect_version:
        raise RepoError(f"{deb.name}: version {info.get('Version')} does not match the tag ({expect_version})")
    description = [(k, v) for k, v in fields if k == "Description"]
    lines = [f"{k}: {v}" for k, v in fields if k != "Description"]
    lines.append(f"Filename: {filename}")
    lines.append(f"Size: {deb.stat().st_size}")
    lines += [f"{k}: {v}" for k, v in file_hashes(deb).items()]
    lines += [f"{k}: {v}" for k, v in description]
    return info["Version"], "\n".join(lines) + "\n"


def gzip_bytes(data: bytes) -> bytes:
    buf = io.BytesIO()
    with gzip.GzipFile(filename="", mode="wb", fileobj=buf, mtime=0, compresslevel=9) as gz:
        gz.write(data)
    return buf.getvalue()


def release_date(iso: str) -> str:
    if iso:
        when = dt.datetime.fromisoformat(iso.replace("Z", "+00:00")).astimezone(dt.timezone.utc)
    elif os.environ.get("SOURCE_DATE_EPOCH"):
        when = dt.datetime.fromtimestamp(int(os.environ["SOURCE_DATE_EPOCH"]), dt.timezone.utc)
    else:
        when = dt.datetime(1970, 1, 1, tzinfo=dt.timezone.utc)
    return when.strftime("%a, %d %b %Y %H:%M:%S UTC")


def release_file(suite: str, date: str, index_files: dict[str, bytes]) -> str:
    lines = [
        f"Origin: {ORIGIN}",
        f"Label: {LABEL}",
        f"Suite: {suite}",
        f"Codename: {suite}",
        f"Date: {date}",
        f"Architectures: {ARCH}",
        f"Components: {COMPONENT}",
        f"Description: ParVu {suite} packages",
    ]
    for title, algo in (("MD5Sum", hashlib.md5), ("SHA256", hashlib.sha256)):
        lines.append(f"{title}:")
        for name in sorted(index_files):
            data = index_files[name]
            lines.append(f" {algo(data).hexdigest()} {len(data):>16} {name}")
    return "\n".join(lines) + "\n"


def gpg(gnupghome: str, *args: str, **kw) -> subprocess.CompletedProcess:
    return run(["gpg", "--homedir", gnupghome, "--batch", "--no-tty", "--pinentry-mode", "error", *args], **kw)


def key_fingerprints(gnupghome: str, *args: str) -> list[str]:
    out = gpg(gnupghome, "--with-colons", "--fingerprint", *args).stdout.decode()
    fprs, primary = [], False
    for line in out.splitlines():
        parts = line.split(":")
        if parts[0] in ("pub", "sec"):
            primary = True
        elif parts[0] == "fpr" and primary:
            fprs.append(parts[9])
            primary = False
        elif parts[0] in ("sub", "ssb"):
            primary = False
    return fprs


def public_keyring(asc: Path, fingerprint: str) -> tuple[bytes, bytes]:
    """The binary keyring and the armoured key, checked against fingerprint."""
    with tempfile.TemporaryDirectory(prefix="parvu-apt-pub-") as home:
        os.chmod(home, 0o700)
        try:
            gpg(home, "--import", str(asc))
        except RepoError as e:
            raise RepoError(f"{asc} is not an OpenPGP public key ({e})") from None
        if gpg(home, "--with-colons", "--list-secret-keys").stdout.strip():
            raise RepoError(f"{asc} contains a secret key; commit only the public key")
        fprs = key_fingerprints(home, "--list-keys")
        if fprs != [fingerprint]:
            raise RepoError(f"{asc} holds the key(s) {fprs or 'none'}, expected exactly {fingerprint}")
        binary = gpg(home, "--export", "--export-options", "export-minimal", fingerprint).stdout
        armoured = gpg(home, "--armor", "--export", "--export-options", "export-minimal", fingerprint).stdout
    return binary, armoured


def sign(gnupghome: str, fingerprint: str, release: Path) -> None:
    common = ["--local-user", fingerprint, "--digest-algo", "SHA512", "--yes"]
    gpg(gnupghome, *common, "--clearsign", "--output", str(release.with_name("InRelease")), str(release))
    gpg(gnupghome, *common, "--armor", "--detach-sign", "--output", str(release.with_name("Release.gpg")), str(release))


def render(template: str, fingerprint: str) -> str:
    text = (TEMPLATES / template).read_text()
    return (text.replace("@SITE_URL@", SITE_URL).replace("@REPO_URL@", REPO_URL)
            .replace("@FINGERPRINT@", fingerprint))


def build(plan: dict, debs: Path, out: Path, public_key: Path, fingerprint: str,
          gnupghome: str | None, unsigned: bool = False) -> None:
    fingerprint = fingerprint.replace(" ", "").upper()
    if not re.fullmatch(r"[0-9A-F]{40}", fingerprint):
        raise RepoError(f"the signing key fingerprint must be 40 hex digits, got {fingerprint!r}")
    if not unsigned:
        gnupghome = gnupghome or os.environ.get("GNUPGHOME")
        if not gnupghome:
            raise RepoError("set --gnupghome or GNUPGHOME to the directory with the signing key")
        if fingerprint not in key_fingerprints(gnupghome, "--list-secret-keys"):
            raise RepoError(f"no secret key {fingerprint} in {gnupghome}: the signing key "
                            "does not match the repository's public key")
    keyring, armoured = public_keyring(public_key, fingerprint)

    releases = {r["tag"]: r for r in plan["releases"]}
    if not any(plan["suites"].get(s) for s in SUITES):
        raise RepoError("the plan has no releases to publish")

    if out.exists():
        shutil.rmtree(out)
    apt = out / "apt"
    pool = apt / "pool" / COMPONENT / PACKAGE[0] / PACKAGE
    pool.mkdir(parents=True)

    stanzas: dict[str, tuple[str, str]] = {}
    for tag, rel in releases.items():
        src = debs / tag / rel["asset"]
        if not src.is_file():
            raise RepoError(f"missing {src}")
        dest = pool / rel["asset"]
        try:
            os.link(src, dest)  # the debs are big; a copy only when linking fails
        except OSError:
            shutil.copyfile(src, dest)
        filename = dest.relative_to(apt).as_posix()
        stanzas[tag] = package_stanza(dest, filename, rel["version"])

    date = release_date(plan.get("date", ""))
    for suite in SUITES:
        tags = plan["suites"].get(suite, [])
        ordered = sorted((stanzas[t] for t in tags), key=lambda s: _version_key(s[0]), reverse=True)
        packages = "\n".join(s[1] for s in ordered).encode()
        suite_dir = apt / "dists" / suite
        index_dir = suite_dir / COMPONENT / f"binary-{ARCH}"
        index_dir.mkdir(parents=True)
        index = {
            f"{COMPONENT}/binary-{ARCH}/Packages": packages,
            f"{COMPONENT}/binary-{ARCH}/Packages.gz": gzip_bytes(packages),
        }
        for name, data in index.items():
            (suite_dir / name).write_bytes(data)
        release = suite_dir / "Release"
        release.write_text(release_file(suite, date, index))
        if not unsigned:
            sign(gnupghome, fingerprint, release)
        print(f"{suite}: {', '.join(tags) or '(empty)'}", file=sys.stderr)

    (apt / "parvu-archive-keyring.gpg").write_bytes(keyring)
    (apt / "parvu-archive-keyring.asc").write_bytes(armoured)
    (apt / "index.html").write_text(render("apt/index.html", fingerprint))
    (out / "index.html").write_text(render("index.html", fingerprint))
    # Deterministic tree: fixed permissions on everything written.
    for p in out.rglob("*"):
        os.chmod(p, 0o755 if p.is_dir() else 0o644)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("plan", help="choose the releases for each suite")
    p.add_argument("--releases", type=Path, required=True, help="GitHub release list (JSON)")
    p.add_argument("--keep", type=int, default=3)
    p.add_argument("--min-version", default="1.0.0")
    p.add_argument("--treat-as-prerelease", action="append", default=[], metavar="TAG",
                   help="handle this tag as a pre-release (for testing)")
    b = sub.add_parser("build", help="write and sign the repository")
    b.add_argument("--plan", type=Path, required=True)
    b.add_argument("--debs", type=Path, required=True, help="folder with <tag>/<asset>.deb")
    b.add_argument("--out", type=Path, required=True, help="site folder to (re)create")
    b.add_argument("--public-key", type=Path, required=True)
    b.add_argument("--fingerprint", required=True)
    b.add_argument("--gnupghome")
    b.add_argument("--unsigned", action="store_true", help="skip signing (for testing)")
    args = ap.parse_args(argv)
    try:
        if args.cmd == "plan":
            plan = make_plan(load_releases(args.releases.read_text()), args.keep,
                             args.min_version, tuple(args.treat_as_prerelease))
            json.dump(plan, sys.stdout, indent=2)
            print()
        else:
            build(json.loads(args.plan.read_text()), args.debs, args.out, args.public_key,
                  args.fingerprint, args.gnupghome, args.unsigned)
    except RepoError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

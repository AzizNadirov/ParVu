# ParVu Releases

## Downloads

| Platform | File | Size |
|---|---|---|
| Linux (Ubuntu/Debian, 64-bit) | `parvu_X.Y.Z_amd64.deb` | about 30 MB |
| Windows 10/11 (64-bit) | `ParVu_X.Y.Z_x64-setup.exe` | about 15 MB |

## Linux

### APT repository (recommended)

On Ubuntu 22.04 / Debian 12 or newer (64-bit), add ParVu's APT repository once; after that `sudo apt upgrade` installs new versions with your other updates:

```bash
sudo install -d -m 0755 /etc/apt/keyrings
curl -fsSL https://aziznadirov.github.io/ParVu/apt/parvu-archive-keyring.gpg | sudo tee /etc/apt/keyrings/parvu-archive-keyring.gpg >/dev/null
echo "Types: deb
URIs: https://aziznadirov.github.io/ParVu/apt
Suites: stable
Components: main
Architectures: amd64
Signed-By: /etc/apt/keyrings/parvu-archive-keyring.gpg" | sudo tee /etc/apt/sources.list.d/parvu.sources >/dev/null
sudo apt update && sudo apt install parvu
```

`install.sh --apt` does the same and checks the key's fingerprint (see [Installer script](#installer-script)).

The repository is signed with the key

```
FINGERPRINT_TBD
```

Check it with `gpg --show-keys /etc/apt/keyrings/parvu-archive-keyring.gpg`.

For pre-releases (alpha, beta, release candidates), write `Suites: beta` instead of `Suites: stable` in `/etc/apt/sources.list.d/parvu.sources`. The beta suite also has the newest stable release, so you get stable updates too. Each suite keeps the three newest versions; older ones are still on the [releases page](https://github.com/AzizNadirov/ParVu/releases).

To stop using the repository: `sudo rm /etc/apt/sources.list.d/parvu.sources /etc/apt/keyrings/parvu-archive-keyring.gpg`.

### A downloaded .deb

```bash
sudo apt install ./parvu_X.Y.Z_amd64.deb
parvu
```

`apt` installs the two system libraries ParVu needs (WebKitGTK and GTK) if they are missing. `sudo dpkg -i` also works when they are already installed. A package installed this way isn't updated by `apt upgrade` unless you add the repository above.

### Installer script

`install.sh` works on Debian, Ubuntu and their derivatives (Debian 12 / Ubuntu 22.04 or newer):

```bash
curl -fsSL https://raw.githubusercontent.com/AzizNadirov/ParVu/main/install.sh | bash -s -- --prebuilt
```

downloads the latest release, checks its SHA-256 and installs it. `--apt` instead adds the APT repository (checking that the downloaded key has the fingerprint above) and installs `parvu` from it, so `apt upgrade` updates it; `--apt --beta` uses the beta suite. Without `--prebuilt` or `--apt` it builds from source instead: it installs the build packages with apt, Rust (in `~/.cargo`) and Node.js (in `~/.cache/parvu/node`) when they are missing or too old, clones the repository to `~/.cache/parvu/src`, runs `build.sh` and installs the result. Run from a checkout (`./install.sh`), it builds that checkout. Other options: `--beta` (build `parvu-next`), `--ref <branch or tag>`, `--src <dir>`, `--dry-run`. The first source build compiles DuckDB and takes 10–20 minutes.

The package replaces any earlier ParVu `.deb`, including the Python versions (0.x). Settings, recent files and themes in `~/.ParVu` are kept.

Uninstall with `sudo apt remove parvu`.

### Using ParVu from AI clients

The `parvu` command also runs an MCP server for Claude Code, Claude Desktop and VS Code (`parvu mcp --allow-dir <folder>`). See [docs/MCP.md](docs/MCP.md).

### Beta builds

Beta packages are named `parvu-next` and install next to `parvu`, so you can try a new version without removing the current one. Start the beta with `parvu-next`; it has its own menu entry. Both share `~/.ParVu`. Remove the beta with `sudo apt remove parvu-next`.

## Windows

Run `ParVu_X.Y.Z_x64-setup.exe` and follow the wizard. It adds a Start menu entry and registers ParVu for Parquet, CSV, JSON and Excel files. Uninstall from **Settings → Apps**.

ParVu uses Microsoft Edge WebView2, which is part of Windows 10 and 11. If it is missing, the installer downloads it.

## Troubleshooting

**Linux: the window is blank or the app does not start.** Run `parvu` from a terminal to see errors. On some graphics drivers this helps:

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 parvu
```

**Windows: "Windows protected your PC".** The installer is not code-signed yet. Click **More info → Run anyway**.

**Logs** are in `~/.ParVu/logs/` on both platforms (`%USERPROFILE%\.ParVu\logs` on Windows). See [docs/CRASH_REPORTING.md](docs/CRASH_REPORTING.md).

## Verifying downloads

```bash
sha256sum parvu_*.deb
```

```powershell
Get-FileHash ParVu_*.exe -Algorithm SHA256
```

Compare with the checksum in the release notes or in `SHA256SUMS.txt`. In release file names, the Debian version's `~` appears as `-` (`parvu_1.0.0-alpha.2_amd64.deb`); the package itself keeps `1.0.0~alpha.2`.

## Building a release

Releases are built and published by GitHub Actions (`.github/workflows/release.yml`):

1. Set the new version in `Cargo.toml`, `crates/parvu-app/tauri.conf.json` and `ui/package.json`, and add a `## [<version>]` section to `CHANGELOG.md`.
2. Merge that to `main`, then tag the commit and push the tag:
   ```bash
   git tag -a v1.0.0 -m "ParVu 1.0.0"
   git push origin v1.0.0
   ```
3. The workflow checks that the tag matches the version, runs the tests, builds the deb and the Windows installer, and publishes a GitHub release with both files, `SHA256SUMS.txt`, and the changelog section as notes. The notes start with install commands for the release's own files. Versions with a `-` (such as `1.1.0-beta.1`) are marked as pre-releases once a stable 1.x release exists; until then each alpha or beta becomes the Latest release, so the releases page doesn't point at the 0.x Python app. A tag that doesn't match the version fails before anything is built.
4. Last, the workflow rebuilds the APT repository (`.github/workflows/apt.yml`, see below) with the new release.

Re-running a failed release workflow replaces the files of the existing release. To build installers locally, see [docs/BUILDING.md](docs/BUILDING.md).

### The APT repository

`.github/workflows/apt.yml` builds the repository from the GitHub releases and deploys it to GitHub Pages (`https://aziznadirov.github.io/ParVu/apt/`). The release workflow calls it after publishing; run it by hand with `gh workflow run "APT repository"` after editing or deleting a release. It:

- picks the releases with `packaging/apt/build_repo.py plan`: `stable` has the three newest releases that are not pre-releases and have no `-` in the tag; `beta` has the newest stable release and the newer ones, three at most. Versions below 1.0.0 are left out;
- downloads each `.deb` and checks it against the release's `SHA256SUMS.txt`;
- writes `Packages`, `Release`, `InRelease` and `Release.gpg` with `build_repo.py build` and signs them with the secret `APT_SIGNING_KEY` (the ASCII-armoured private key, no passphrase). It stops if that key's fingerprint isn't the one in `apt.yml`, or if `packaging/apt/parvu-archive-keyring.asc` holds another key.

The release is published before this job starts and doesn't depend on it: if the secret is missing the job fails with a message, and the release stays as it is.

To try the generator locally: `python3 -m unittest discover -s packaging/apt`, then `build_repo.py plan --releases <file>` on the output of `gh api --paginate "repos/AzizNadirov/ParVu/releases?per_page=100" --jq '.[]'`, and `build_repo.py build` with a throwaway key in its own `GNUPGHOME` (or `--unsigned`).

Changing the signing key means changing the fingerprint in `RELEASES.md`, `install.sh` and `.github/workflows/apt.yml`, and the public key in `packaging/apt/parvu-archive-keyring.asc`, in one commit. Users then have to download the new keyring again.

## Support

- [README.md](README.md)
- [Issues](https://github.com/AzizNadirov/ParVu/issues)

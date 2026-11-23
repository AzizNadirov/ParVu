# File Associations

ParVu can open data files straight from the file manager.

## File types

| Type | Extensions |
|---|---|
| Parquet | `.parquet`, `.pq` |
| CSV | `.csv`, `.tsv` |
| JSON | `.json`, `.jsonl`, `.ndjson` |
| Excel | `.xlsx`, `.xls` |

## Linux (Ubuntu/Debian)

Installing ParVu with apt (from the APT repository or a downloaded `.deb`, see [RELEASES.md](../RELEASES.md#linux)) registers it for these types and adds a Parquet MIME type, so ParVu appears under **Open With**.

The snap registers ParVu for CSV, JSON and Excel files. A snap can't add MIME types, so `.parquet` files only offer ParVu when the system already knows the Parquet type (for example after installing the deb once); in the snap the desktop file is `parvu_parvu.desktop`.

### Make ParVu the default

**From the file manager:** right-click a file → **Open With Other Application** (GNOME) or **Properties → File Type Options** (KDE) → choose **ParVu** → set as default.

**From the command line:**

```bash
xdg-mime default parvu.desktop application/vnd.apache.parquet
xdg-mime default parvu.desktop application/x-parquet
xdg-mime default parvu.desktop text/csv            # optional
xdg-mime default parvu.desktop application/json    # optional
xdg-mime query default application/vnd.apache.parquet   # prints parvu.desktop
```

The beta package installs as `parvu-next.desktop`; use that name instead if you installed the beta.

### ParVu is missing from Open With

```bash
sudo update-desktop-database /usr/share/applications
sudo update-mime-database /usr/share/mime
```

Then log out and back in.

## Windows

The `setup.exe` installer registers ParVu for these file types.

To make it the default: right-click a file → **Open with → Choose another app** → **ParVu** → tick **Always use this app**. Or use **Settings → Apps → Default apps → Choose defaults by file type**.

## Opening files

- Double-click a file once ParVu is the default.
- Select several files and open them together; each opens in its own tab.
- From a terminal: `parvu sales.parquet customers.csv`.
- Drop files onto an open ParVu window.

## Removing the associations

Uninstalling ParVu removes them: `sudo apt remove parvu` on Linux, or **Settings → Apps** on Windows. To keep ParVu but stop it being the default, choose another app as described above.

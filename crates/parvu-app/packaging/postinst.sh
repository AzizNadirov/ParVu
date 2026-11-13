#!/bin/sh
set -e
command -v update-mime-database >/dev/null && update-mime-database /usr/share/mime || true
command -v update-desktop-database >/dev/null && update-desktop-database /usr/share/applications || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -f /usr/share/icons/hicolor || true
exit 0

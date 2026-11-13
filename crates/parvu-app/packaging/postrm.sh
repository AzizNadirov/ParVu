#!/bin/sh
set -e
if [ "$1" = "remove" ] || [ "$1" = "purge" ]; then
  command -v update-mime-database >/dev/null && update-mime-database /usr/share/mime || true
  command -v update-desktop-database >/dev/null && update-desktop-database /usr/share/applications || true
fi
exit 0

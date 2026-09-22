#!/bin/sh
set -eu
if [ "${RENEWED_LINEAGE:-}" = /etc/letsencrypt/live/anydrop-api.vanillacake.cn ]; then
  nginx -t
  systemctl reload nginx
fi

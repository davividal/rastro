#!/usr/bin/env bash
# One line per captured server: what a reviewer checks first.
set -u

field() {
  local reply=$1 name=$2
  tr -d '\r' < "$reply" 2>/dev/null | sed -n "s/^$name://p" | head -1
  return 0
}

acl_entries() {
  local reply=$1
  head -c 20 "$reply" 2>/dev/null | sed -n '1s/^\*\([0-9-]*\).*/\1/p'
  return 0
}

for capture in /captures/redis/[0-9][0-9]; do
  cell=$(basename "$capture")
  if [[ -f "$capture/SETUP_FAILED" ]]; then
    echo "$cell SETUP FAILED"
    continue
  fi
  echo "$cell servers=$(cat "$capture/server-count") rastro root=$(cat "$capture/exit-root") unprivileged=$(cat "$capture/exit-unprivileged")"
  for server in "$capture"/server-[0-9]*; do
    [[ -d "$server" ]] || continue
    info=$(ls "$server"/replies/authenticated/*INFO-server.resp 2>/dev/null | head -1)
    noauth=$(head -c 40 "$server"/replies/unauthenticated/*.resp 2>/dev/null | tr -d '\r\n')
    echo "  $(basename "$server") comm=$(cat "$server/process/comm") unit=$(cat "$server/unit" 2>/dev/null || echo -)" \
      "name=$(field "$info" server_name) redis=$(field "$info" redis_version) valkey=$(field "$info" valkey_version)" \
      "unauthenticated=[$noauth] acl_log_after=$(acl_entries "$(ls "$server"/cost/after/*ACL-LOG.resp 2>/dev/null | head -1)")" \
      "log_lines_during=$(wc -l < "$server/cost/log-during-rastro.txt" 2>/dev/null || echo -)"
  done
done

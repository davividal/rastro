#!/usr/bin/env bash
# One line per captured node: what a reviewer checks first.
set -u

readonly STATUSES=(root nodes_local_settings cluster_settings index_template ilm_policy ingest_pipeline mapping)

summarise_node() {
  local cell=$1 node=$2 jar port noauth statuses request parent yml
  jar=$(sed 's/elasticsearch-//; s/.jar//' "$node/files/version-jar" 2>/dev/null | head -1)
  port=$(cat "$node/http-port" 2>/dev/null || echo "-")
  noauth=$(cat "$node/api/root.noauth.status" 2>/dev/null)
  statuses=""
  for request in "${STATUSES[@]}"; do
    statuses="$statuses $request=$(cut -d' ' -f1 "$node/api/$request.status" 2>/dev/null)"
  done
  parent=$(tr '\0' ' ' < "$node/parent/cmdline" 2>/dev/null \
    | grep -oE 'CliToolLauncher|server-launcher|tini|systemd-entrypoint|docker-entrypoint' | head -1)
  if [[ -s "$node/files/elasticsearch.yml" ]]; then yml=yml; else yml=no-yml; fi
  echo "$cell $(basename "$node") v=$jar port=[$port] noauth=$noauth$statuses parent=${parent:-$(cat "$node/parent.pid")} $yml"
  return 0
}

for capture in /captures/*; do
  cell=$(basename "$capture")
  if [[ -f "$capture/SETUP_FAILED" ]]; then
    echo "$cell SETUP FAILED"
    continue
  fi
  if [[ "$(cat "$capture/node-count" 2>/dev/null)" == 0 ]]; then
    echo "$cell no node"
    continue
  fi
  for node in "$capture"/node-[0-9]*; do
    summarise_node "$cell" "$node"
  done
done

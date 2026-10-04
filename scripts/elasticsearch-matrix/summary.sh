#!/usr/bin/env bash
# One line per captured node: what a reviewer checks first.
for c in /captures/*; do
  cell=$(basename "$c")
  [ -f "$c/SETUP_FAILED" ] && { echo "$cell SETUP FAILED"; continue; }
  n=$(cat "$c/node-count" 2>/dev/null); [ "$n" = 0 ] && { echo "$cell no node"; continue; }
  for node in "$c"/node-[0-9]*; do
    jar=$(sed 's/elasticsearch-//; s/.jar//' "$node/files/version-jar" 2>/dev/null | head -1)
    port=$(cat "$node/http-port" 2>/dev/null || echo "-")
    noauth=$(cat "$node/api/root.noauth.status" 2>/dev/null)
    st=""; for r in root nodes_local_settings cluster_settings index_template ilm_policy ingest_pipeline mapping; do st="$st $r=$(cut -d' ' -f1 "$node/api/$r.status" 2>/dev/null)"; done
    parent=$(tr '\0' ' ' < "$node/parent/cmdline" 2>/dev/null | grep -oE 'CliToolLauncher|server-launcher|tini|systemd-entrypoint|docker-entrypoint' | head -1)
    yml=$( [ -s "$node/files/elasticsearch.yml" ] && echo yml || echo "no-yml" )
    echo "$cell $(basename $node) v=$jar port=[$port] noauth=$noauth$st parent=${parent:-$(cat $node/parent.pid)} $yml"
  done
done

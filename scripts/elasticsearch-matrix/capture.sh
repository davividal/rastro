#!/usr/bin/env bash
# Captures what rastro would see of every Elasticsearch node on this box, for one matrix cell.
# Usage: capture.sh CELL   (reads /root/cells/CELL.env: PORTS="9200=https ...", AUTH=(curl args))
set -u

readonly SERVER_CLASSES='org.elasticsearch.bootstrap.Elasticsearch\|org.elasticsearch.server/org.elasticsearch.bootstrap.Elasticsearch'
readonly PROC_FILES=(cmdline environ stat status cgroup mountinfo net/tcp net/tcp6)
readonly PROC_LINKS=(cwd root exe ns/net ns/mnt ns/pid)
readonly REQUESTS=(
  "root|/"
  "cluster_settings|/_cluster/settings?flat_settings=true"
  "index_template|/_index_template"
  "component_template|/_component_template"
  "alias|/*,-.*/_alias?expand_wildcards=open,closed"
  "index_settings|/*,-.*/_settings?flat_settings=true&expand_wildcards=open,closed"
  "mapping|/*,-.*/_mapping?expand_wildcards=open,closed"
  "ilm_policy|/_ilm/policy"
  "ingest_pipeline|/_ingest/pipeline"
  "snapshot|/_snapshot"
  "plugins|/_nodes/_local/plugins"
  "nodes_local|/_nodes/_local?flat_settings=true&filter_path=nodes.*.settings,nodes.*.roles,nodes.*.attributes,nodes.*.jvm.input_arguments,nodes.*.jvm.mem.heap_max_in_bytes"
  "nodes_local_settings|/_nodes/_local/settings?flat_settings=true"
  "health_local|/_cluster/health?local=true"
  "blocks_local|/_cluster/state/blocks?local=true"
)

is_server() {
  local process=$1
  tr '\0' '\n' < "$process/cmdline" 2>/dev/null | grep -qx "$SERVER_CLASSES"
  return $?
}

# Copies a process's files and links, and notes which an unprivileged account may read.
copy_proc() {
  local pid=$1 dir=$2 file link readable
  mkdir -p "$dir"
  for file in "${PROC_FILES[@]}"; do
    mkdir -p "$dir/$(dirname "$file")"
    cat "/proc/$pid/$file" > "$dir/$file" 2>/dev/null || echo "unreadable" > "$dir/$file.error"
  done
  for link in "${PROC_LINKS[@]}"; do
    mkdir -p "$dir/$(dirname "$link")"
    readlink "/proc/$pid/$link" > "$dir/$link.link" 2>/dev/null || echo unreadable > "$dir/$link.error"
  done
  # Every target whole, a ` (deleted)` marker included: a jar replaced under a running node.
  for descriptor in "/proc/$pid/fd"/*; do
    echo "${descriptor##*/} -> $(readlink "$descriptor")"
  done > "$dir/fd.list" 2>/dev/null
  : > "$dir/unprivileged.txt"
  for file in "${PROC_FILES[@]}" fd; do
    if runuser -u nobody -- sh -c "ls /proc/$pid/$file >/dev/null 2>&1 && cat /proc/$pid/$file >/dev/null 2>&1"; then
      readable=readable
    else
      readable=refused
    fi
    echo "$file $readable" >> "$dir/unprivileged.txt"
  done
  return 0
}

# The config directory inside the node's root, as the launchers resolve it.
config_dir() {
  local pid=$1 home conf
  home=$(tr '\0' '\n' < "/proc/$pid/cmdline" | sed -n 's/^-Des.path.home=//p')
  conf=$(tr '\0' '\n' < "/proc/$pid/cmdline" | sed -n 's/^-Des.path.conf=//p')
  [[ -z "$conf" ]] && conf=$(tr '\0' '\n' < "/proc/$pid/environ" | sed -n 's/^ES_PATH_CONF=//p')
  [[ -z "$conf" ]] && conf=$home/config
  echo "$conf"
  return 0
}

# Gives an open node one of everything the facet reports.
seed() {
  local pid=$1 url=$2 node=$3
  put() {
    local path=$1 body=$2
    nsenter -t "$pid" -n curl -sk -m 40 "${AUTH[@]}" -XPUT -H 'Content-Type: application/json' \
      -o /dev/null -w "PUT $path %{http_code}\n" "$url$path" -d "$body"
    return 0
  }
  {
    put /_component_template/app-mappings '{"template":{"mappings":{"properties":{"title":{"type":"text"},"score":{"type":"float"}}}}}'
    put /_index_template/app '{"index_patterns":["myapp-*"],"composed_of":["app-mappings"],"template":{"settings":{"number_of_shards":1,"number_of_replicas":0}},"priority":100}'
    put /myapp-tenant1_1790000000 '{"aliases":{"myapp-tenant1":{}}}'
    put /unaliased '{"settings":{"number_of_replicas":0}}'
    put /_ilm/policy/app-policy '{"policy":{"phases":{"delete":{"min_age":"30d","actions":{"delete":{}}}}}}'
    put /_ingest/pipeline/app-pipeline '{"processors":[{"set":{"field":"env","value":"prod"}}]}'
    put /_cluster/settings '{"persistent":{"cluster.routing.allocation.enable":"all"}}'
  } > "$node/seed.log" 2>&1
  return 0
}

# Asks the node every request in the fixed list, and once more without credentials.
ask() {
  local pid=$1 url=$2 node=$3 request name path
  mkdir -p "$node/api"
  nsenter -t "$pid" -n curl -sk -m 20 -o "$node/api/root.noauth.body" -w '%{http_code}\n' "$url/" \
    > "$node/api/root.noauth.status"
  for request in "${REQUESTS[@]}"; do
    name=${request%%|*}
    path=${request#*|}
    nsenter -t "$pid" -n curl -sk -m 40 "${AUTH[@]}" -D "$node/api/$name.headers" \
      -o "$node/api/$name.body" -w '%{http_code} %{time_total}\n' "$url$path" > "$node/api/$name.status"
  done
  return 0
}

# Whether this node itself holds a listener on `port`: in a shared netns another node may.
holds_port() {
  local port=$1 node=$2 hex inodes inode
  hex=$(printf '%04X' "$port")
  grep -qE ":$hex [0-9A-F:]+ 0A " "$node/server/net/tcp" "$node/server/net/tcp6" 2>/dev/null || return 1
  inodes=$(awk -v h=":$hex" '$2 ~ h"$" && $4=="0A" {print $10}' "$node/server/net/tcp" "$node/server/net/tcp6")
  for inode in $inodes; do
    grep -q "socket:\[$inode\]" "$node/server/fd.list" && return 0
  done
  return 1
}

capture_node() {
  local pid=$1 node=$2 ppid conf home entry port scheme url
  copy_proc "$pid" "$node/server"
  ppid=$(awk '{print $4}' "/proc/$pid/stat")
  echo "$ppid" > "$node/parent.pid"
  [[ "$ppid" -gt 1 ]] && copy_proc "$ppid" "$node/parent"
  conf=$(config_dir "$pid")
  echo "$conf" > "$node/config-dir"
  mkdir -p "$node/files"
  nsenter -t "$pid" -m ls -la "$conf" > "$node/files/config-dir.ls" 2>&1
  nsenter -t "$pid" -m cat "$conf/elasticsearch.yml" > "$node/files/elasticsearch.yml" 2>"$node/files/elasticsearch.yml.error"
  home=$(tr '\0' '\n' < "/proc/$pid/cmdline" | sed -n 's/^-Des.path.home=//p')
  nsenter -t "$pid" -m sh -c "ls $home/lib | grep -E '^elasticsearch-[0-9]'" > "$node/files/version-jar" 2>&1
  for entry in $PORTS; do
    port=${entry%%=*}
    scheme=${entry#*=}
    holds_port "$port" "$node" || continue
    echo "$port $scheme" > "$node/http-port"
    url="$scheme://127.0.0.1:$port"
    [[ "${SEED:-1}" == 1 ]] && seed "$pid" "$url" "$node"
    ask "$pid" "$url" "$node"
  done
  [[ -f "$node/http-port" ]] || echo "no cell port matched a listener of pid $pid" > "$node/http-port.error"
  return 0
}

main() {
  local cell=$1 out process found=0
  out=/captures/$cell
  rm -rf "$out"
  mkdir -p "$out"
  PORTS=""
  AUTH=()
  # shellcheck source=/dev/null
  [[ -f "/root/cells/$cell.env" ]] && . "/root/cells/$cell.env"

  { echo "cell $cell"; date -u +%FT%TZ; uname -a; echo "ports: $PORTS"; echo "auth: ${#AUTH[@]} args"; } > "$out/meta.txt"
  cat /proc/1/mountinfo > "$out/host-mountinfo"
  readlink /proc/1/ns/net > "$out/host-netns.link"
  if command -v docker >/dev/null; then
    docker ps --format '{{.ID}} {{.Names}} {{.Image}}' > "$out/containers.txt" 2>/dev/null
    for id in $(docker ps -q 2>/dev/null); do docker inspect "$id" > "$out/docker-inspect-$id.json"; done
  fi

  for process in /proc/[0-9]*; do
    is_server "$process" || continue
    found=$((found + 1))
    capture_node "${process#/proc/}" "$out/node-$found"
  done
  echo "$found" > "$out/node-count"
  echo "captured $found node(s) into $out"
  return 0
}

main "$@"

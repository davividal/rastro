#!/usr/bin/env bash
# Captures what rastro would see of every Elasticsearch node on this box, for one matrix cell.
# Usage: capture.sh CELL   (reads /root/cells/CELL.env: PORTS="9200=https ...", AUTH=(curl args))
set -u
cell=$1
out=/captures/$cell
rm -rf "$out"; mkdir -p "$out"
PORTS=""; AUTH=()
[ -f /root/cells/$cell.env ] && . /root/cells/$cell.env

REQUESTS=(
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
)

is_server() { tr '\0' '\n' < "$1/cmdline" 2>/dev/null | grep -qx 'org.elasticsearch.bootstrap.Elasticsearch\|org.elasticsearch.server/org.elasticsearch.bootstrap.Elasticsearch'; }

copy_proc() { # pid dir
  local pid=$1 dir=$2
  mkdir -p "$dir"
  for f in cmdline environ stat status cgroup mountinfo net/tcp net/tcp6; do
    mkdir -p "$dir/$(dirname "$f")"; cat "/proc/$pid/$f" > "$dir/$f" 2>/dev/null || echo "unreadable" > "$dir/$f.error"
  done
  for l in cwd root exe ns/net ns/mnt ns/pid; do
    mkdir -p "$dir/$(dirname "$l")"; readlink "/proc/$pid/$l" > "$dir/$l.link" 2>/dev/null || echo unreadable > "$dir/$l.error"
  done
  ls -l "/proc/$pid/fd" 2>/dev/null | awk '{print $9, $10, $11}' > "$dir/fd.list"
  : > "$dir/unprivileged.txt"
  for f in cmdline environ stat status cgroup mountinfo net/tcp net/tcp6 fd; do
    if runuser -u nobody -- sh -c "ls /proc/$pid/$f >/dev/null 2>&1 && cat /proc/$pid/$f >/dev/null 2>&1"; then r=readable; else r=refused; fi
    echo "$f $r" >> "$dir/unprivileged.txt"
  done
}

config_dir() { # pid -> config dir inside the node's root
  local pid=$1 home conf
  home=$(tr '\0' '\n' < /proc/$pid/cmdline | sed -n 's/^-Des.path.home=//p')
  conf=$(tr '\0' '\n' < /proc/$pid/cmdline | sed -n 's/^-Des.path.conf=//p')
  [ -z "$conf" ] && conf=$(tr '\0' '\n' < /proc/$pid/environ | sed -n 's/^ES_PATH_CONF=//p')
  [ -z "$conf" ] && conf=$home/config
  echo "$conf"
}

{ echo "cell $cell"; date -u +%FT%TZ; uname -a; echo "ports: $PORTS"; echo "auth: ${#AUTH[@]} args"; } > "$out/meta.txt"
cat /proc/1/mountinfo > "$out/host-mountinfo"
readlink /proc/1/ns/net > "$out/host-netns.link"
command -v docker >/dev/null && docker ps --format '{{.ID}} {{.Names}} {{.Image}}' > "$out/containers.txt" 2>/dev/null
for id in $(docker ps -q 2>/dev/null); do docker inspect "$id" > "$out/docker-inspect-$id.json"; done

found=0
for p in /proc/[0-9]*; do
  is_server "$p" || continue
  pid=${p#/proc/}; found=$((found+1))
  node="$out/node-$found"
  copy_proc "$pid" "$node/server"
  ppid=$(awk '{print $4}' /proc/$pid/stat)
  echo "$ppid" > "$node/parent.pid"
  [ "$ppid" -gt 1 ] && copy_proc "$ppid" "$node/parent"
  conf=$(config_dir "$pid"); echo "$conf" > "$node/config-dir"
  mkdir -p "$node/files"
  nsenter -t "$pid" -m ls -la "$conf" > "$node/files/config-dir.ls" 2>&1
  nsenter -t "$pid" -m cat "$conf/elasticsearch.yml" > "$node/files/elasticsearch.yml" 2>"$node/files/elasticsearch.yml.error"
  home=$(tr '\0' '\n' < /proc/$pid/cmdline | sed -n 's/^-Des.path.home=//p')
  nsenter -t "$pid" -m sh -c "ls $home/lib | grep -E '^elasticsearch-[0-9]'" > "$node/files/version-jar" 2>&1
  # The node's HTTP port: whichever of the cell's ports it listens on in its own netns.
  for entry in $PORTS; do
    port=${entry%%=*}; scheme=${entry#*=}
    hex=$(printf '%04X' "$port")
    grep -qE ":$hex [0-9A-F:]+ 0A " "$node/server/net/tcp" "$node/server/net/tcp6" 2>/dev/null || continue
    # In a shared netns the port may belong to another node: match the socket inode to this pid.
    inodes=$(awk -v h=":$hex" '$2 ~ h"$" && $4=="0A" {print $10}' "$node/server/net/tcp" "$node/server/net/tcp6")
    mine=0; for i in $inodes; do grep -q "socket:\[$i\]" "$node/server/fd.list" && mine=1; done
    [ $mine = 1 ] || continue
    echo "$port $scheme" > "$node/http-port"
    mkdir -p "$node/api"
    url="$scheme://127.0.0.1:$port"
    if [ "${SEED:-1}" = 1 ]; then
      put() { nsenter -t "$pid" -n curl -sk -m 40 "${AUTH[@]}" -XPUT -H 'Content-Type: application/json' -o /dev/null -w "PUT $1 %{http_code}\n" "$url$1" -d "$2"; }
      {
        put /_component_template/app-mappings '{"template":{"mappings":{"properties":{"title":{"type":"text"},"score":{"type":"float"}}}}}'
        put /_index_template/app '{"index_patterns":["myapp-*"],"composed_of":["app-mappings"],"template":{"settings":{"number_of_shards":1,"number_of_replicas":0}},"priority":100}'
        put /myapp-tenant1_1790000000 '{"aliases":{"myapp-tenant1":{}}}'
        put /unaliased '{"settings":{"number_of_replicas":0}}'
        put /_ilm/policy/app-policy '{"policy":{"phases":{"delete":{"min_age":"30d","actions":{"delete":{}}}}}}'
        put /_ingest/pipeline/app-pipeline '{"processors":[{"set":{"field":"env","value":"prod"}}]}'
        put /_cluster/settings '{"persistent":{"cluster.routing.allocation.enable":"all"}}'
      } > "$node/seed.log" 2>&1
    fi
    nsenter -t "$pid" -n curl -sk -m 20 -o "$node/api/root.noauth.body" -w '%{http_code}\n' "$url/" > "$node/api/root.noauth.status"
    for r in "${REQUESTS[@]}"; do
      name=${r%%|*}; path=${r#*|}
      nsenter -t "$pid" -n curl -sk -m 40 "${AUTH[@]}" -D "$node/api/$name.headers" -o "$node/api/$name.body" -w '%{http_code} %{time_total}\n' "$url$path" > "$node/api/$name.status"
    done
  done
  [ -f "$node/http-port" ] || echo "no cell port matched a listener of pid $pid" > "$node/http-port.error"
done
echo "$found" > "$out/node-count"
echo "captured $found node(s) into $out"

#!/usr/bin/env bash
# Matrix cell setups. Usage: cells.sh prepare | cells.sh reset | cells.sh run NN
# One cell at a time: `run` resets the box, sets the cell up, captures it, collects logs.
set -u

readonly DL=/root/dl
readonly HEAP='ES_JAVA_OPTS=-Xms512m -Xmx512m'
readonly ART=https://artifacts.elastic.co/downloads/elasticsearch
readonly IMG=docker.elastic.co/elasticsearch/elasticsearch
readonly DEBS="7.17.29 8.19.22 9.5.4 8.15.3 9.4.7"
readonly TARS="9.4.7 8.19.22 7.17.29 9.5.4"
readonly IMAGES="$IMG:7.17.29 $IMG:8.19.22 $IMG:9.4.7 $IMG:9.5.4 $IMG:9.2.0 docker.elastic.co/elasticsearch/elasticsearch-oss:7.10.2"
readonly PACKAGE_FILE=/etc/elasticsearch/elasticsearch.yml

# The capture's view of a cell: which ports serve HTTP, in which protocol, with which credential.
readonly PLAIN_9200='PORTS="9200=http"'
readonly TLS_9200_WITH_KEY='PORTS="9200=https"\nAUTH=(-H "Authorization: ApiKey %s")\n'

log() {
  echo "[$(date +%T)] $*" >&2
  return 0
}

# Downloads over HTTPS only, redirects included.
fetch() {
  local url=$1 file=$2
  [[ -f "$file" ]] || curl -sfL --proto '=https' --proto-redir '=https' -o "$file" "$url"
  return $?
}

prepare() {
  local loop version image
  mkdir -p "$DL" /root/cells /captures
  DEBIAN_FRONTEND=noninteractive apt-get -qqy install lvm2 util-linux procps >/dev/null
  id es >/dev/null 2>&1 || useradd -r -m -s /bin/bash es
  if ! mountpoint -q /data; then
    if [[ -f /var/lib/data.img ]]; then
      losetup -j /var/lib/data.img | grep -q . || losetup --find /var/lib/data.img
      vgchange -q -ay vgdata >/dev/null
    else
      truncate -s 20G /var/lib/data.img
      loop=$(losetup --find --show /var/lib/data.img)
      pvcreate -q "$loop"
      vgcreate -q vgdata "$loop"
      lvcreate -q -n data -l 100%FREE vgdata
      mkfs.ext4 -q /dev/vgdata/data
    fi
    mkdir -p /data
    mount /dev/vgdata/data /data
  fi
  for version in $DEBS; do fetch "$ART/elasticsearch-$version-arm64.deb" "$DL/es-$version.deb"; done
  for version in $TARS; do fetch "$ART/elasticsearch-$version-linux-aarch64.tar.gz" "$DL/es-$version.tgz"; done
  for image in $IMAGES; do docker pull -q "$image" >/dev/null; done
  docker pull -q --platform linux/amd64 "$IMG:6.8.23" >/dev/null
  ls -la "$DL"
  docker images --format '{{.Repository}}:{{.Tag}} {{.Size}}'
  return 0
}

reset() {
  # shellcheck disable=SC2046
  docker rm -f $(docker ps -aq) >/dev/null 2>&1
  # shellcheck disable=SC2046
  docker volume rm $(docker volume ls -q) >/dev/null 2>&1
  docker network prune -f >/dev/null 2>&1
  systemctl stop es-tar elasticsearch 2>/dev/null
  rm -f /etc/systemd/system/es-tar.service
  systemctl daemon-reload
  pkill -9 -u es 2>/dev/null
  sleep 2
  if dpkg -s elasticsearch >/dev/null 2>&1; then
    DEBIAN_FRONTEND=noninteractive apt-get -qqy purge elasticsearch >/dev/null 2>&1
  fi
  rm -rf /etc/elasticsearch /var/lib/elasticsearch /var/log/elasticsearch /usr/share/elasticsearch /etc/default/elasticsearch
  rm -rf /opt/es-* /srv/es* /data/* /home/es/*.pid
  return 0
}

# Polls `url` until anything answers; further arguments go to curl.
wait_url() {
  local url=$1 code
  shift
  for _ in $(seq 1 200); do
    code=$(curl -sk -m 5 -o /dev/null -w '%{http_code}' "$@" "$url" 2>/dev/null)
    if [[ "$code" != 000 ]]; then
      log "$url answered $code"
      return 0
    fi
    sleep 3
  done
  log "$url never answered"
  return 1
}

wait_container() {
  local name=$1 port=$2 scheme=$3 code
  for _ in $(seq 1 200); do
    code=$(docker exec "$name" curl -sk -m 5 -o /dev/null -w '%{http_code}' "$scheme://127.0.0.1:$port/" 2>/dev/null)
    if [[ -n "$code" && "$code" != 000 ]]; then
      log "$name answered $code"
      return 0
    fi
    sleep 3
  done
  log "$name never answered"
  docker logs "$name" 2>&1 | tail -20
  return 1
}

# Mints an API key from `url`; further arguments authenticate the request.
api_key() {
  local url=$1
  shift
  curl -sk "$@" -XPOST -H 'Content-Type: application/json' "$url/_security/api_key" \
    -d '{"name":"rastro-capture"}' | sed -n 's/.*"encoded":"\([^"]*\)".*/\1/p'
  return 0
}

container_key() {
  local name=$1 password=$2
  docker exec "$name" curl -sk -u "elastic:$password" -XPOST -H 'Content-Type: application/json' \
    https://127.0.0.1:9200/_security/api_key -d '{"name":"rastro-capture"}' \
    | sed -n 's/.*"encoded":"\([^"]*\)".*/\1/p'
  return 0
}

env_file() {
  local cell=$1
  cat > "/root/cells/$cell.env"
  return 0
}

with_key() {
  local cell=$1 key=$2
  # shellcheck disable=SC2059
  printf "$TLS_9200_WITH_KEY" "$key" | env_file "$cell"
  return 0
}

deb_install() {
  local version=$1
  DEBIAN_FRONTEND=noninteractive dpkg -i "$DL/es-$version.deb" > "/root/cells/install-$version.log" 2>&1
  return $?
}

deb_security_off() {
  sed -i 's/^xpack.security.enabled: true/xpack.security.enabled: false/; s/^  enabled: true/  enabled: false/; s/^xpack.security.enrollment.enabled: true/xpack.security.enrollment.enabled: false/; /^cluster.initial_master_nodes/d' "$PACKAGE_FILE"
  return 0
}

# Switches TLS off on HTTP alone, in the nested block auto-configuration writes.
http_tls_off() {
  local file=$1
  awk '/^xpack.security.http.ssl:/{f=1} f&&/^  enabled: true/{sub("true","false");f=0} {print}' "$file" > /tmp/y
  cat /tmp/y > "$file"
  return 0
}

deb_heap() {
  mkdir -p /etc/elasticsearch/jvm.options.d
  printf -- '-Xms512m\n-Xmx512m\n' > /etc/elasticsearch/jvm.options.d/heap.options
  chown -R root:elasticsearch /etc/elasticsearch/jvm.options.d
  return 0
}

tar_extract() {
  local version=$1 dir=$2
  mkdir -p "$dir"
  tar -xzf "$DL/es-$version.tgz" -C "$dir" --strip-components=1
  mkdir -p "$dir/config/jvm.options.d"
  printf -- '-Xms512m\n-Xmx512m\n' > "$dir/config/jvm.options.d/heap.options"
  chown -R es:es "$dir"
  return 0
}

as_es() {
  runuser -u es -- "$@"
  return $?
}

cell_01() {
  deb_install 7.17.29 && deb_heap && systemctl start elasticsearch || return 1
  wait_url http://127.0.0.1:9200/ || return 1
  env_file 01 <<<"$PLAIN_9200"
  return 0
}

cell_02() {
  deb_install 8.19.22 && deb_heap && systemctl start elasticsearch || return 1
  wait_url https://127.0.0.1:9200/ || return 1
  env_file 02 <<<'PORTS="9200=https"'
  return 0
}

cell_03() {
  deb_install 8.19.22 && deb_heap && deb_security_off || return 1
  mkdir -p /data/es && chown elasticsearch:elasticsearch /data/es
  sed -i 's|^path.data: .*|path.data: /data/es|' "$PACKAGE_FILE"
  printf 'network.host: 0.0.0.0\ndiscovery.type: single-node\n' >> "$PACKAGE_FILE"
  systemctl start elasticsearch && wait_url http://127.0.0.1:9200/ || return 1
  env_file 03 <<<"$PLAIN_9200"
  return 0
}

cell_04() {
  deb_install 9.5.4 && deb_heap && deb_security_off || return 1
  mkdir -p /srv/es
  cp -a /etc/elasticsearch /srv/es/config
  echo 'http.port: 9205' >> /srv/es/config/elasticsearch.yml
  echo 'ES_PATH_CONF=/srv/es/config' >> /etc/default/elasticsearch
  systemctl start elasticsearch && wait_url http://127.0.0.1:9205/ || return 1
  env_file 04 <<<'PORTS="9205=http"'
  return 0
}

cell_05() {
  tar_extract 9.4.7 /opt/es-05
  as_es /opt/es-05/bin/elasticsearch -d -p /home/es/es05.pid -E node.name=cell05 \
    -E discovery.type=single-node -E xpack.security.enabled=false || return 1
  wait_url http://127.0.0.1:9200/ || return 1
  env_file 05 <<<"$PLAIN_9200"
  return 0
}

cell_06() {
  tar_extract 8.19.22 /opt/es-06
  # First start auto-configures security and TLS into the file; stopped once it has.
  as_es sh -c 'cd /opt/es-06 && nohup bin/elasticsearch > /home/es/es06-first.log 2>&1 & echo $! > /home/es/es06-first.pid'
  wait_url https://127.0.0.1:9200/ || return 1
  pkill -u es java
  for _ in $(seq 1 60); do
    pgrep -u es java >/dev/null || break
    sleep 2
  done
  http_tls_off /opt/es-06/config/elasticsearch.yml
  as_es /opt/es-06/bin/elasticsearch -d -p /home/es/es06.pid -E xpack.security.http.ssl.enabled=true || return 1
  wait_url https://127.0.0.1:9200/ || return 1
  # What rastro decides from the file: plain HTTP. The capture records the wrong-protocol answer.
  env_file 06 <<<"$PLAIN_9200"
  return 0
}

cell_07() {
  tar_extract 7.17.29 /opt/es-07
  mkdir -p /data/es07 /srv/es07b
  chown es:es /data/es07 /srv/es07b
  echo 'path.data: [/data/es07, /srv/es07b]' >> /opt/es-07/config/elasticsearch.yml
  as_es /opt/es-07/bin/elasticsearch -d -p /home/es/es07.pid -E node.name=cell07 -E discovery.type=single-node || return 1
  wait_url http://127.0.0.1:9200/ || return 1
  env_file 07 <<<"$PLAIN_9200"
  return 0
}

cell_08() {
  tar_extract 9.5.4 /opt/es-08
  mkdir -p /srv/es08conf /data/es08
  mv /opt/es-08/config/elasticsearch.yml /srv/es08conf/elasticsearch.yml
  printf 'xpack.security.enabled: false\ndiscovery.type: single-node\npath.data: /data/es08\n' >> /srv/es08conf/elasticsearch.yml
  ln -s /srv/es08conf/elasticsearch.yml /opt/es-08/config/elasticsearch.yml
  chown -R es:es /srv/es08conf /data/es08 /opt/es-08
  cat > /etc/systemd/system/es-tar.service <<'UNIT'
[Unit]
Description=Elasticsearch from a tarball
[Service]
User=es
ExecStart=/opt/es-08/bin/elasticsearch -E node.name=cell08 -E http.port=9208
LimitNOFILE=65535
[Install]
WantedBy=multi-user.target
UNIT
  systemctl daemon-reload
  systemctl start es-tar && wait_url http://127.0.0.1:9208/ || return 1
  env_file 08 <<<'PORTS="9208=http"'
  return 0
}

cell_09() {
  docker run -d --name cell09 -e "$HEAP" -e discovery.type=single-node -e xpack.security.enabled=false \
    -v cell09data:/usr/share/elasticsearch/data "$IMG:9.5.4" >/dev/null || return 1
  wait_container cell09 9200 http || return 1
  env_file 09 <<<"$PLAIN_9200"
  return 0
}

cell_10() {
  mkdir -p /srv/es10 /data/es10
  docker create --name tmp10 "$IMG:8.19.22" >/dev/null
  docker cp tmp10:/usr/share/elasticsearch/config /srv/es10/config
  docker rm tmp10 >/dev/null
  printf 'cluster.name: cell10\nnetwork.host: 0.0.0.0\ndiscovery.type: single-node\nxpack.security.enabled: false\n' > /srv/es10/config/elasticsearch.yml
  chown -R 1000:0 /srv/es10 /data/es10
  docker run -d --name cell10 -e "$HEAP" -v /srv/es10/config:/usr/share/elasticsearch/config \
    -v /data/es10:/usr/share/elasticsearch/data "$IMG:8.19.22" >/dev/null || return 1
  wait_container cell10 9200 http || return 1
  env_file 10 <<<"$PLAIN_9200"
  return 0
}

cell_11() {
  docker build -q -t cell11 - >/dev/null <<IMAGE
FROM $IMG:9.4.7
USER root
RUN mkdir -p /srv/es-config \
 && printf 'network.host: 0.0.0.0\nhttp.port: 9350\ndiscovery.type: single-node\nxpack.security.enabled: false\n' > /srv/es-config/elasticsearch.yml \
 && rm /usr/share/elasticsearch/config/elasticsearch.yml \
 && chown -R 1000:0 /srv/es-config \
 && ln -s /srv/es-config/elasticsearch.yml /usr/share/elasticsearch/config/elasticsearch.yml
USER 1000:0
IMAGE
  docker run -d --name cell11 -e "$HEAP" cell11 eswrapper -E node.name=cell11 -E http.port=9351 >/dev/null || return 1
  wait_container cell11 9351 http || return 1
  env_file 11 <<<'PORTS="9350=http 9351=http"'
  return 0
}

cell_12() {
  local key
  docker run -d --name cell12 -e "$HEAP" -e ELASTIC_PASSWORD=cell12-pass "$IMG:8.19.22" >/dev/null || return 1
  wait_container cell12 9200 https || return 1
  key=$(container_key cell12 cell12-pass)
  with_key 12 "$key"
  return 0
}

cell_13() {
  local name
  docker network create cell13 >/dev/null
  for name in cell13a cell13b; do
    docker run -d --name "$name" --network cell13 -e "$HEAP" -e "node.name=$name" -e cluster.name=cell13 \
      -e discovery.seed_hosts=cell13a,cell13b -e cluster.initial_master_nodes=cell13a,cell13b \
      -e xpack.security.enabled=false -v "${name}data:/usr/share/elasticsearch/data" "$IMG:9.5.4" >/dev/null || return 1
  done
  wait_container cell13a 9200 http && wait_container cell13b 9200 http || return 1
  for _ in $(seq 1 40); do
    docker exec cell13a curl -s 127.0.0.1:9200/_cat/nodes | grep -c cell13 | grep -q 2 && break
    sleep 3
  done
  env_file 13 <<<"$PLAIN_9200"
  return 0
}

cell_14() {
  local node
  for node in a b; do tar_extract 7.17.29 "/opt/es-14$node"; done
  as_es /opt/es-14a/bin/elasticsearch -d -p /home/es/es14a.pid -E node.name=n14a -E cluster.name=cell14 \
    -E http.port=9200 -E transport.port=9300 \
    -E discovery.seed_hosts=127.0.0.1:9300,127.0.0.1:9301 -E cluster.initial_master_nodes=n14a,n14b || return 1
  as_es /opt/es-14b/bin/elasticsearch -d -p /home/es/es14b.pid -E node.name=n14b -E cluster.name=cell14 \
    -E http.port=9201 -E transport.port=9301 \
    -E discovery.seed_hosts=127.0.0.1:9300,127.0.0.1:9301 -E cluster.initial_master_nodes=n14a,n14b || return 1
  wait_url http://127.0.0.1:9200/ && wait_url http://127.0.0.1:9201/ || return 1
  for _ in $(seq 1 40); do
    curl -s 127.0.0.1:9200/_cat/nodes | grep -c n14 | grep -q 2 && break
    sleep 3
  done
  env_file 14 <<<'PORTS="9200=http 9201=http"'
  return 0
}

cell_15() {
  docker run -d --name cell15 -e "$HEAP" -e node.name=cell15 -e cluster.initial_master_nodes=ghost-node \
    -e xpack.security.enabled=false "$IMG:8.19.22" >/dev/null || return 1
  wait_container cell15 9200 http || return 1
  sleep 20
  printf '%s\nSEED=0\n' "$PLAIN_9200" | env_file 15
  return 0
}

cell_16() {
  docker run -d --name cell16 --platform linux/amd64 -e "$HEAP" -e discovery.type=single-node "$IMG:6.8.23" >/dev/null || return 1
  wait_container cell16 9200 http || return 1
  env_file 16 <<<"$PLAIN_9200"
  return 0
}

cell_17() {
  docker run -d --name cell17 -e "$HEAP" -e discovery.type=single-node \
    docker.elastic.co/elasticsearch/elasticsearch-oss:7.10.2 >/dev/null || return 1
  wait_container cell17 9200 http || return 1
  env_file 17 <<<"$PLAIN_9200"
  return 0
}

cell_18() {
  deb_install 8.15.3 && deb_heap && deb_security_off || return 1
  echo 'discovery.type: single-node' >> "$PACKAGE_FILE"
  systemctl start elasticsearch && wait_url http://127.0.0.1:9200/ || return 1
  env_file 18 <<<"$PLAIN_9200"
  return 0
}

cell_19() {
  docker run -d --name cell19 -e "$HEAP" -e discovery.type=single-node -e xpack.security.enabled=false \
    "$IMG:9.2.0" >/dev/null || return 1
  wait_container cell19 9200 http || return 1
  env_file 19 <<<"$PLAIN_9200"
  return 0
}

cell_20() {
  deb_install 9.5.4 || return 1
  env_file 20 <<<"$PLAIN_9200"
  return 0
}

cell_21() {
  local pass key
  deb_install 8.19.22 && deb_heap && systemctl start elasticsearch || return 1
  wait_url https://127.0.0.1:9200/ || return 1
  pass=$(/usr/share/elasticsearch/bin/elasticsearch-reset-password -u elastic -b -s 2>/dev/null | tail -1)
  key=$(api_key https://127.0.0.1:9200 -u "elastic:$pass")
  docker run -d --name cell21 -e "$HEAP" -e ELASTIC_PASSWORD=cell21-pass "$IMG:9.5.4" >/dev/null || return 1
  wait_container cell21 9200 https || return 1
  with_key 21 "$key"
  return 0
}

cell_22() {
  local name key
  for name in cell22a cell22b; do
    docker run -d --name "$name" -e "$HEAP" -e "ELASTIC_PASSWORD=$name-pass" \
      -v "${name}data:/usr/share/elasticsearch/data" "$IMG:9.5.4" >/dev/null || return 1
  done
  wait_container cell22a 9200 https && wait_container cell22b 9200 https || return 1
  key=$(container_key cell22a cell22a-pass)
  with_key 22 "$key"
  return 0
}

cell_23() {
  local node
  for node in a b; do
    tar_extract 7.17.29 "/opt/es-23$node"
    as_es sh -c "cd /opt/es-23$node && bin/elasticsearch-keystore create >/dev/null && echo cell23$node-pass | bin/elasticsearch-keystore add -x bootstrap.password"
  done
  as_es /opt/es-23a/bin/elasticsearch -d -p /home/es/es23a.pid -E cluster.name=cell23a -E discovery.type=single-node \
    -E xpack.security.enabled=true -E http.port=9200 -E transport.port=9300 || return 1
  as_es /opt/es-23b/bin/elasticsearch -d -p /home/es/es23b.pid -E cluster.name=cell23b -E discovery.type=single-node \
    -E xpack.security.enabled=true -E http.port=9201 -E transport.port=9301 || return 1
  wait_url http://127.0.0.1:9200/ && wait_url http://127.0.0.1:9201/ || return 1
  printf 'PORTS="9200=http 9201=http"\nAUTH=(-u elastic:cell23a-pass)\n' | env_file 23
  return 0
}

cell_24() {
  local key
  docker run -d --name mint24 -e "$HEAP" -e ELASTIC_PASSWORD=mint-pass "$IMG:9.5.4" >/dev/null || return 1
  wait_container mint24 9200 https || return 1
  key=$(container_key mint24 mint-pass)
  docker rm -f mint24 >/dev/null
  docker run -d --name cell24 -e "$HEAP" -e ELASTIC_PASSWORD=cell24-pass \
    -v cell24data:/usr/share/elasticsearch/data "$IMG:9.5.4" >/dev/null || return 1
  wait_container cell24 9200 https || return 1
  with_key 24 "$key"
  return 0
}

cell_25() {
  local pass
  deb_install 9.4.7 && deb_heap || return 1
  http_tls_off "$PACKAGE_FILE"
  systemctl start elasticsearch && wait_url http://127.0.0.1:9200/ || return 1
  pass=$(/usr/share/elasticsearch/bin/elasticsearch-reset-password -u elastic -b -s 2>/dev/null | tail -1)
  printf '%s\nAUTH=(-u "elastic:%s")\n' "$PLAIN_9200" "$pass" | env_file 25
  return 0
}

cell_26() {
  local key
  docker run -d --name cell26 -e "$HEAP" -e ELASTIC_PASSWORD=cell26-pass -e xpack.security.audit.enabled=true \
    "$IMG:8.19.22" >/dev/null || return 1
  wait_container cell26 9200 https || return 1
  key=$(container_key cell26 cell26-pass)
  with_key 26 "$key"
  return 0
}

# After the capture, so the logs hold what the capture caused.
collect_logs() {
  local cell=$1 out container file
  out=/captures/$cell
  mkdir -p "$out/logs"
  sleep 5
  for container in $(docker ps -a --format '{{.Names}}'); do
    docker logs "$container" > "$out/logs/docker-$container.log" 2>&1
  done
  journalctl -u elasticsearch -u es-tar --no-pager > "$out/logs/journal.log" 2>/dev/null
  for file in /var/log/elasticsearch/*.json /var/log/elasticsearch/*.log /opt/es-*/logs/*.json /opt/es-*/logs/*.log; do
    [[ -f "$file" ]] && cp "$file" "$out/logs/$(echo "${file#/}" | tr / _)"
  done
  cp /root/cells/install-*.log "$out/logs/" 2>/dev/null
  rm -f /root/cells/install-*.log
  return 0
}

run_cell() {
  local cell=$1
  reset
  log "cell $cell: setup"
  if "cell_$cell"; then
    log "cell $cell: capture"
    /root/capture.sh "$cell"
  else
    log "cell $cell: SETUP FAILED"
    mkdir -p "/captures/$cell"
    echo "setup failed" > "/captures/$cell/SETUP_FAILED"
  fi
  collect_logs "$cell"
  if [[ "$(cat "/captures/$cell/node-count" 2>/dev/null)" == 0 && "$cell" != 20 ]]; then
    log "cell $cell: NO NODE CAPTURED"
  fi
  return 0
}

main() {
  local command=${1:-} cell=${2:-}
  case "$command" in
    prepare) prepare ;;
    reset) reset ;;
    run) run_cell "$cell" ;;
    *)
      echo "usage: cells.sh prepare | cells.sh reset | cells.sh run NN" >&2
      return 2
      ;;
  esac
  return 0
}

main "$@"

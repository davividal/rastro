#!/usr/bin/env bash
# Matrix cell setups. Usage: cells.sh prepare | cells.sh run NN
# One cell at a time: `run` resets the box, sets the cell up, captures it, collects logs.
set -u
DL=/root/dl
HEAP='ES_JAVA_OPTS=-Xms512m -Xmx512m'
ART=https://artifacts.elastic.co/downloads/elasticsearch
IMG=docker.elastic.co/elasticsearch/elasticsearch
DEBS="7.17.29 8.19.22 9.5.4 8.15.3 9.4.7"
TARS="9.4.7 8.19.22 7.17.29 9.5.4"
IMAGES="$IMG:7.17.29 $IMG:8.19.22 $IMG:9.4.7 $IMG:9.5.4 $IMG:9.2.0 docker.elastic.co/elasticsearch/elasticsearch-oss:7.10.2"

log() { echo "[$(date +%T)] $*" >&2; }

prepare() {
  mkdir -p $DL /root/cells /captures
  DEBIAN_FRONTEND=noninteractive apt-get -qqy install lvm2 util-linux procps >/dev/null
  id es >/dev/null 2>&1 || useradd -r -m -s /bin/bash es
  if ! mountpoint -q /data; then
    if [ -f /var/lib/data.img ]; then
      losetup -j /var/lib/data.img | grep -q . || losetup --find /var/lib/data.img
      vgchange -q -ay vgdata >/dev/null
    else
      truncate -s 20G /var/lib/data.img
      loop=$(losetup --find --show /var/lib/data.img)
      pvcreate -q "$loop"; vgcreate -q vgdata "$loop"; lvcreate -q -n data -l 100%FREE vgdata
      mkfs.ext4 -q /dev/vgdata/data
    fi
    mkdir -p /data; mount /dev/vgdata/data /data
  fi
  for v in $DEBS; do [ -f $DL/es-$v.deb ] || curl -sfL -o $DL/es-$v.deb $ART/elasticsearch-$v-arm64.deb; done
  for v in $TARS; do [ -f $DL/es-$v.tgz ] || curl -sfL -o $DL/es-$v.tgz $ART/elasticsearch-$v-linux-aarch64.tar.gz; done
  for i in $IMAGES; do docker pull -q "$i" >/dev/null; done
  docker pull -q --platform linux/amd64 $IMG:6.8.23 >/dev/null
  ls -la $DL; docker images --format '{{.Repository}}:{{.Tag}} {{.Size}}'
}

reset() {
  docker rm -f $(docker ps -aq) >/dev/null 2>&1
  docker volume rm $(docker volume ls -q) >/dev/null 2>&1; docker network prune -f >/dev/null 2>&1
  systemctl stop es-tar elasticsearch 2>/dev/null; rm -f /etc/systemd/system/es-tar.service; systemctl daemon-reload
  pkill -9 -u es 2>/dev/null; sleep 2
  if dpkg -s elasticsearch >/dev/null 2>&1; then DEBIAN_FRONTEND=noninteractive apt-get -qqy purge elasticsearch >/dev/null 2>&1; fi
  rm -rf /etc/elasticsearch /var/lib/elasticsearch /var/log/elasticsearch /usr/share/elasticsearch /etc/default/elasticsearch
  rm -rf /opt/es-* /srv/es* /data/* /home/es/*.pid
}

wait_url() { # url [curl args...]; until anything answers
  local url=$1; shift
  for _ in $(seq 1 200); do
    code=$(curl -sk -m 5 -o /dev/null -w '%{http_code}' "$@" "$url" 2>/dev/null)
    [ "$code" != 000 ] && { log "$url answered $code"; return 0; }
    sleep 3
  done
  log "$url never answered"; return 1
}
wait_container() { # name port scheme
  for _ in $(seq 1 200); do
    code=$(docker exec "$1" curl -sk -m 5 -o /dev/null -w '%{http_code}' "$3://127.0.0.1:$2/" 2>/dev/null)
    [ -n "$code" ] && [ "$code" != 000 ] && { log "$1 answered $code"; return 0; }
    sleep 3
  done
  log "$1 never answered"; docker logs "$1" 2>&1 | tail -20; return 1
}
api_key() { # url curl-auth-args... -> encoded key
  local url=$1; shift
  curl -sk "$@" -XPOST -H 'Content-Type: application/json' "$url/_security/api_key" -d '{"name":"rastro-capture"}' | sed -n 's/.*"encoded":"\([^"]*\)".*/\1/p'
}
container_key() { # container password
  docker exec "$1" curl -sk -u "elastic:$2" -XPOST -H 'Content-Type: application/json' https://127.0.0.1:9200/_security/api_key -d '{"name":"rastro-capture"}' | sed -n 's/.*"encoded":"\([^"]*\)".*/\1/p'
}
env_file() { cat > /root/cells/$1.env; }
deb_install() { DEBIAN_FRONTEND=noninteractive dpkg -i $DL/es-$1.deb > /root/cells/install-$1.log 2>&1; }
deb_security_off() {
  sed -i 's/^xpack.security.enabled: true/xpack.security.enabled: false/; s/^  enabled: true/  enabled: false/; s/^xpack.security.enrollment.enabled: true/xpack.security.enrollment.enabled: false/; /^cluster.initial_master_nodes/d' /etc/elasticsearch/elasticsearch.yml
}
deb_http_tls_off() { awk '/^xpack.security.http.ssl:/{f=1} f&&/^  enabled: true/{sub("true","false");f=0} {print}' /etc/elasticsearch/elasticsearch.yml > /tmp/y && cat /tmp/y > /etc/elasticsearch/elasticsearch.yml; }
deb_heap() { mkdir -p /etc/elasticsearch/jvm.options.d; printf -- '-Xms512m\n-Xmx512m\n' > /etc/elasticsearch/jvm.options.d/heap.options; chown -R root:elasticsearch /etc/elasticsearch/jvm.options.d; }
tar_extract() { # version dir
  mkdir -p "$2"; tar -xzf $DL/es-$1.tgz -C "$2" --strip-components=1
  mkdir -p "$2/config/jvm.options.d"; printf -- '-Xms512m\n-Xmx512m\n' > "$2/config/jvm.options.d/heap.options"
  chown -R es:es "$2"
}
as_es() { runuser -u es -- "$@"; }

cell_01() { deb_install 7.17.29; deb_heap; systemctl start elasticsearch; wait_url http://127.0.0.1:9200/; env_file 01 <<<'PORTS="9200=http"'; }
cell_02() { deb_install 8.19.22; deb_heap; systemctl start elasticsearch; wait_url https://127.0.0.1:9200/; env_file 02 <<<'PORTS="9200=https"'; }
cell_03() {
  deb_install 8.19.22; deb_heap; deb_security_off
  mkdir -p /data/es && chown elasticsearch:elasticsearch /data/es
  sed -i 's|^path.data: .*|path.data: /data/es|' /etc/elasticsearch/elasticsearch.yml
  printf 'network.host: 0.0.0.0\ndiscovery.type: single-node\n' >> /etc/elasticsearch/elasticsearch.yml
  systemctl start elasticsearch; wait_url http://127.0.0.1:9200/; env_file 03 <<<'PORTS="9200=http"'
}
cell_04() {
  deb_install 9.5.4; deb_heap; deb_security_off
  mkdir -p /srv/es; cp -a /etc/elasticsearch /srv/es/config
  echo 'http.port: 9205' >> /srv/es/config/elasticsearch.yml
  echo 'ES_PATH_CONF=/srv/es/config' >> /etc/default/elasticsearch
  systemctl start elasticsearch; wait_url http://127.0.0.1:9205/; env_file 04 <<<'PORTS="9205=http"'
}
cell_05() {
  tar_extract 9.4.7 /opt/es-05
  as_es /opt/es-05/bin/elasticsearch -d -p /home/es/es05.pid -E node.name=cell05 -E discovery.type=single-node -E xpack.security.enabled=false
  wait_url http://127.0.0.1:9200/; env_file 05 <<<'PORTS="9200=http"'
}
cell_06() {
  tar_extract 8.19.22 /opt/es-06
  # First start auto-configures security and TLS into the file; stopped once it has.
  as_es sh -c 'cd /opt/es-06 && nohup bin/elasticsearch > /home/es/es06-first.log 2>&1 & echo $! > /home/es/es06-first.pid'
  wait_url https://127.0.0.1:9200/ || return 1; pkill -u es java
  for _ in $(seq 1 60); do pgrep -u es java >/dev/null || break; sleep 2; done
  awk '/^xpack.security.http.ssl:/{f=1} f&&/^  enabled: true/{sub("true","false");f=0} {print}' /opt/es-06/config/elasticsearch.yml > /tmp/y && cat /tmp/y > /opt/es-06/config/elasticsearch.yml
  as_es /opt/es-06/bin/elasticsearch -d -p /home/es/es06.pid -E xpack.security.http.ssl.enabled=true
  wait_url https://127.0.0.1:9200/ || return 1
  # What rastro decides from the file: plain HTTP. The capture records the wrong-protocol answer.
  env_file 06 <<<'PORTS="9200=http"'
}
cell_07() {
  tar_extract 7.17.29 /opt/es-07; mkdir -p /data/es07 /srv/es07b; chown es:es /data/es07 /srv/es07b
  echo 'path.data: [/data/es07, /srv/es07b]' >> /opt/es-07/config/elasticsearch.yml
  as_es /opt/es-07/bin/elasticsearch -d -p /home/es/es07.pid -E node.name=cell07 -E discovery.type=single-node
  wait_url http://127.0.0.1:9200/; env_file 07 <<<'PORTS="9200=http"'
}
cell_08() {
  tar_extract 9.5.4 /opt/es-08; mkdir -p /srv/es08conf /data/es08
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
  systemctl daemon-reload; systemctl start es-tar; wait_url http://127.0.0.1:9208/; env_file 08 <<<'PORTS="9208=http"'
}
cell_09() {
  docker run -d --name cell09 -e "$HEAP" -e discovery.type=single-node -e xpack.security.enabled=false -v cell09data:/usr/share/elasticsearch/data $IMG:9.5.4 >/dev/null
  wait_container cell09 9200 http; env_file 09 <<<'PORTS="9200=http"'
}
cell_10() {
  mkdir -p /srv/es10 /data/es10
  docker create --name tmp10 $IMG:8.19.22 >/dev/null; docker cp tmp10:/usr/share/elasticsearch/config /srv/es10/config; docker rm tmp10 >/dev/null
  printf 'cluster.name: cell10\nnetwork.host: 0.0.0.0\ndiscovery.type: single-node\nxpack.security.enabled: false\n' > /srv/es10/config/elasticsearch.yml
  chown -R 1000:0 /srv/es10 /data/es10
  docker run -d --name cell10 -e "$HEAP" -v /srv/es10/config:/usr/share/elasticsearch/config -v /data/es10:/usr/share/elasticsearch/data $IMG:8.19.22 >/dev/null
  wait_container cell10 9200 http; env_file 10 <<<'PORTS="9200=http"'
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
  docker run -d --name cell11 -e "$HEAP" cell11 eswrapper -E node.name=cell11 -E http.port=9351 >/dev/null
  wait_container cell11 9351 http; env_file 11 <<<'PORTS="9350=http 9351=http"'
}
cell_12() {
  docker run -d --name cell12 -e "$HEAP" -e ELASTIC_PASSWORD=cell12-pass $IMG:8.19.22 >/dev/null
  wait_container cell12 9200 https
  key=$(container_key cell12 cell12-pass)
  printf 'PORTS="9200=https"\nAUTH=(-H "Authorization: ApiKey %s")\n' "$key" | env_file 12
}
cell_13() {
  local n
  docker network create cell13 >/dev/null
  for n in cell13a cell13b; do
    docker run -d --name $n --network cell13 -e "$HEAP" -e node.name=$n -e cluster.name=cell13 \
      -e discovery.seed_hosts=cell13a,cell13b -e cluster.initial_master_nodes=cell13a,cell13b \
      -e xpack.security.enabled=false -v ${n}data:/usr/share/elasticsearch/data $IMG:9.5.4 >/dev/null
  done
  wait_container cell13a 9200 http; wait_container cell13b 9200 http
  for _ in $(seq 1 40); do docker exec cell13a curl -s 127.0.0.1:9200/_cat/nodes | grep -c cell13 | grep -q 2 && break; sleep 3; done
  env_file 13 <<<'PORTS="9200=http"'
}
cell_14() {
  local n
  for n in a b; do tar_extract 7.17.29 /opt/es-14$n; done
  as_es /opt/es-14a/bin/elasticsearch -d -p /home/es/es14a.pid -E node.name=n14a -E cluster.name=cell14 -E http.port=9200 -E transport.port=9300 \
    -E discovery.seed_hosts=127.0.0.1:9300,127.0.0.1:9301 -E cluster.initial_master_nodes=n14a,n14b
  as_es /opt/es-14b/bin/elasticsearch -d -p /home/es/es14b.pid -E node.name=n14b -E cluster.name=cell14 -E http.port=9201 -E transport.port=9301 \
    -E discovery.seed_hosts=127.0.0.1:9300,127.0.0.1:9301 -E cluster.initial_master_nodes=n14a,n14b
  wait_url http://127.0.0.1:9200/; wait_url http://127.0.0.1:9201/
  for _ in $(seq 1 40); do curl -s 127.0.0.1:9200/_cat/nodes | grep -c n14 | grep -q 2 && break; sleep 3; done
  env_file 14 <<<'PORTS="9200=http 9201=http"'
}
cell_15() {
  docker run -d --name cell15 -e "$HEAP" -e node.name=cell15 -e cluster.initial_master_nodes=ghost-node -e xpack.security.enabled=false $IMG:8.19.22 >/dev/null
  wait_container cell15 9200 http; sleep 20; env_file 15 <<<'PORTS="9200=http"
SEED=0'
}
cell_16() {
  docker run -d --name cell16 --platform linux/amd64 -e "$HEAP" -e discovery.type=single-node $IMG:6.8.23 >/dev/null
  wait_container cell16 9200 http; env_file 16 <<<'PORTS="9200=http"'
}
cell_17() {
  docker run -d --name cell17 -e "$HEAP" -e discovery.type=single-node docker.elastic.co/elasticsearch/elasticsearch-oss:7.10.2 >/dev/null
  wait_container cell17 9200 http; env_file 17 <<<'PORTS="9200=http"'
}
cell_18() { deb_install 8.15.3; deb_heap; deb_security_off; echo 'discovery.type: single-node' >> /etc/elasticsearch/elasticsearch.yml; systemctl start elasticsearch; wait_url http://127.0.0.1:9200/; env_file 18 <<<'PORTS="9200=http"'; }
cell_19() {
  docker run -d --name cell19 -e "$HEAP" -e discovery.type=single-node -e xpack.security.enabled=false $IMG:9.2.0 >/dev/null
  wait_container cell19 9200 http; env_file 19 <<<'PORTS="9200=http"'
}
cell_20() { deb_install 9.5.4; env_file 20 <<<'PORTS="9200=http"'; }
cell_21() {
  deb_install 8.19.22; deb_heap; systemctl start elasticsearch; wait_url https://127.0.0.1:9200/
  pass=$(/usr/share/elasticsearch/bin/elasticsearch-reset-password -u elastic -b -s 2>/dev/null | tail -1)
  key=$(api_key https://127.0.0.1:9200 -u "elastic:$pass")
  docker run -d --name cell21 -e "$HEAP" -e ELASTIC_PASSWORD=cell21-pass $IMG:9.5.4 >/dev/null
  wait_container cell21 9200 https
  printf 'PORTS="9200=https"\nAUTH=(-H "Authorization: ApiKey %s")\n' "$key" | env_file 21
}
cell_22() {
  local n
  for n in cell22a cell22b; do docker run -d --name $n -e "$HEAP" -e ELASTIC_PASSWORD=$n-pass -v ${n}data:/usr/share/elasticsearch/data $IMG:9.5.4 >/dev/null; done
  wait_container cell22a 9200 https; wait_container cell22b 9200 https
  key=$(container_key cell22a cell22a-pass)
  printf 'PORTS="9200=https"\nAUTH=(-H "Authorization: ApiKey %s")\n' "$key" | env_file 22
}
cell_23() {
  local n
  for n in a b; do
    tar_extract 7.17.29 /opt/es-23$n
    as_es sh -c "cd /opt/es-23$n && bin/elasticsearch-keystore create >/dev/null && echo cell23$n-pass | bin/elasticsearch-keystore add -x bootstrap.password"
  done
  as_es /opt/es-23a/bin/elasticsearch -d -p /home/es/es23a.pid -E cluster.name=cell23a -E discovery.type=single-node -E xpack.security.enabled=true -E http.port=9200 -E transport.port=9300
  as_es /opt/es-23b/bin/elasticsearch -d -p /home/es/es23b.pid -E cluster.name=cell23b -E discovery.type=single-node -E xpack.security.enabled=true -E http.port=9201 -E transport.port=9301
  wait_url http://127.0.0.1:9200/; wait_url http://127.0.0.1:9201/
  env_file 23 <<<'PORTS="9200=http 9201=http"
AUTH=(-u elastic:cell23a-pass)'
}
cell_24() {
  docker run -d --name mint24 -e "$HEAP" -e ELASTIC_PASSWORD=mint-pass $IMG:9.5.4 >/dev/null
  wait_container mint24 9200 https; key=$(container_key mint24 mint-pass); docker rm -f mint24 >/dev/null
  docker run -d --name cell24 -e "$HEAP" -e ELASTIC_PASSWORD=cell24-pass -v cell24data:/usr/share/elasticsearch/data $IMG:9.5.4 >/dev/null
  wait_container cell24 9200 https
  printf 'PORTS="9200=https"\nAUTH=(-H "Authorization: ApiKey %s")\n' "$key" | env_file 24
}
cell_25() {
  deb_install 9.4.7; deb_heap; deb_http_tls_off; systemctl start elasticsearch; wait_url http://127.0.0.1:9200/
  pass=$(/usr/share/elasticsearch/bin/elasticsearch-reset-password -u elastic -b -s 2>/dev/null | tail -1)
  printf 'PORTS="9200=http"\nAUTH=(-u "elastic:%s")\n' "$pass" | env_file 25
}
cell_26() {
  docker run -d --name cell26 -e "$HEAP" -e ELASTIC_PASSWORD=cell26-pass -e xpack.security.audit.enabled=true $IMG:8.19.22 >/dev/null
  wait_container cell26 9200 https
  key=$(container_key cell26 cell26-pass)
  printf 'PORTS="9200=https"\nAUTH=(-H "Authorization: ApiKey %s")\n' "$key" | env_file 26
}

collect_logs() { # after the capture, so the logs hold what the capture caused
  local out=/captures/$1; mkdir -p "$out/logs"; sleep 5
  for c in $(docker ps -a --format '{{.Names}}'); do docker logs "$c" > "$out/logs/docker-$c.log" 2>&1; done
  journalctl -u elasticsearch -u es-tar --no-pager > "$out/logs/journal.log" 2>/dev/null
  for f in /var/log/elasticsearch/*.json /var/log/elasticsearch/*.log /opt/es-*/logs/*.json /opt/es-*/logs/*.log; do
    [ -f "$f" ] && cp "$f" "$out/logs/$(echo "${f#/}" | tr / _)"
  done
  cp /root/cells/install-*.log "$out/logs/" 2>/dev/null; rm -f /root/cells/install-*.log
}

case "$1" in
  prepare) prepare ;;
  reset) reset ;;
  run)
    cell=$2; reset; log "cell $cell: setup"
    if "cell_$cell"; then log "cell $cell: capture"; /root/capture.sh "$cell"; else log "cell $cell: SETUP FAILED"; mkdir -p /captures/$cell; echo "setup failed" > /captures/$cell/SETUP_FAILED; fi
    collect_logs "$cell"
    [ "$(cat /captures/$cell/node-count 2>/dev/null)" = 0 ] && [ "$cell" != 20 ] && log "cell $cell: NO NODE CAPTURED"
    ;;
esac

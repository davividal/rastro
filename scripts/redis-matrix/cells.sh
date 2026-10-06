#!/usr/bin/env bash
# Matrix cell setups. Usage: cells.sh prepare | cells.sh reset | cells.sh run NN
# One cell at a time: `run` resets the box, sets the cell up, captures it, collects logs.
#
# Every password here is invented for the matrix, and the capture keeps it in the fixtures.
set -u

readonly SRC=/root/src
readonly REDIS_IO_KEY=https://packages.redis.io/gpg
readonly REDIS_IO_LIST='deb [signed-by=/usr/share/keyrings/redis-archive-keyring.gpg] https://packages.redis.io/deb bookworm main'
readonly IMAGES="redis:8.10.2 redis:8.8.3 valkey/valkey:9.1.2 valkey/valkey:7.2.14"
readonly VALKEY_BUILDS="9.0.6 8.1.10 8.0.11"
readonly REDIS_5=5.0.14
readonly REDIS_5_SHA256=3ea5024766d983249e80d4aa9457c897a9f079957d0fb1f35682df233f997f32
readonly HTTPS_ONLY='=https'
readonly CONF=/etc/redis/redis.conf
readonly PACKAGE_LOG=/var/log/redis/redis-server.log

log() {
  echo "[$(date +%T)] $*" >&2
  return 0
}

fetch() {
  local url=$1 file=$2
  [[ -f "$file" ]] && return 0
  curl -sfL --proto "$HTTPS_ONLY" --proto-redir "$HTTPS_ONLY" -o "$file.part" "$url" || return 1
  mv "$file.part" "$file"
  return 0
}

# valkey publishes no checksum for its source archives, so these are taken from GitHub over HTTPS.
build_valkey() {
  local version=$1
  [[ -x "/opt/valkey-$version/bin/valkey-server" ]] && return 0
  fetch "https://github.com/valkey-io/valkey/archive/refs/tags/$version.tar.gz" "$SRC/valkey-$version.tgz" || return 1
  rm -rf "$SRC/valkey-$version"
  tar -C "$SRC" -xzf "$SRC/valkey-$version.tgz"
  make -s -C "$SRC/valkey-$version" -j"$(nproc)" BUILD_TLS=no >/dev/null 2>&1 || return 1
  make -s -C "$SRC/valkey-$version" PREFIX="/opt/valkey-$version" install >/dev/null 2>&1
  return 0
}

build_redis_5() {
  [[ -x "/opt/redis-$REDIS_5/bin/redis-server" ]] && return 0
  fetch "https://download.redis.io/releases/redis-$REDIS_5.tar.gz" "$SRC/redis-$REDIS_5.tgz" || return 1
  echo "$REDIS_5_SHA256  $SRC/redis-$REDIS_5.tgz" | sha256sum -c --quiet - || return 1
  rm -rf "$SRC/redis-$REDIS_5"
  tar -C "$SRC" -xzf "$SRC/redis-$REDIS_5.tgz"
  make -s -C "$SRC/redis-$REDIS_5" -j"$(nproc)" MALLOC=libc >/dev/null 2>&1 || return 1
  make -s -C "$SRC/redis-$REDIS_5" PREFIX="/opt/redis-$REDIS_5" install >/dev/null 2>&1
  return 0
}

prepare() {
  local image version
  mkdir -p "$SRC" /root/cells /captures/redis
  DEBIAN_FRONTEND=noninteractive apt-get -qqy install build-essential pkg-config libssl-dev \
    openssl python3 util-linux procps gnupg >/dev/null
  install -d /usr/share/keyrings
  curl -fsSL --proto "$HTTPS_ONLY" "$REDIS_IO_KEY" | gpg --dearmor --yes -o /usr/share/keyrings/redis-archive-keyring.gpg
  echo "$REDIS_IO_LIST" > /etc/apt/sources.list.d/redis.list
  apt-get -qq update
  mountpoint -q /data || log "no /data: cell 16 needs the LVM volume the elasticsearch matrix made"
  for version in $VALKEY_BUILDS; do build_valkey "$version" || log "valkey $version did not build"; done
  build_redis_5 || log "redis $REDIS_5 did not build"
  for image in $IMAGES; do docker pull -q "$image" >/dev/null || log "$image did not pull"; done
  id valkey >/dev/null 2>&1 || useradd -r -M -s /usr/sbin/nologin valkey
  id redis5 >/dev/null 2>&1 || useradd -r -M -s /usr/sbin/nologin redis5
  ls /opt
  docker images --format '{{.Repository}}:{{.Tag}} {{.Size}}'
  return 0
}

reset() {
  local unit
  # shellcheck disable=SC2046
  docker rm -f $(docker ps -aq) >/dev/null 2>&1
  # shellcheck disable=SC2046
  docker volume rm $(docker volume ls -q) >/dev/null 2>&1
  for unit in redis-server redis-server@queue redis-server@cache redis-sentinel valkey-27 valkey-28 valkey-29 redis5; do
    systemctl stop "$unit" 2>/dev/null
  done
  rm -f /etc/systemd/system/valkey-2?.service /etc/systemd/system/redis5.service
  rm -rf /etc/systemd/system/redis-server.service.d
  systemctl daemon-reload
  pkill -9 -x redis-server 2>/dev/null
  pkill -9 -x valkey-server 2>/dev/null
  pkill -9 -x redis-sentinel 2>/dev/null
  sleep 1
  if dpkg -s redis-server >/dev/null 2>&1 || dpkg -s redis-sentinel >/dev/null 2>&1; then
    DEBIAN_FRONTEND=noninteractive apt-get -qqy purge redis-server redis-sentinel redis-tools >/dev/null 2>&1
  fi
  rm -rf /etc/redis /var/lib/redis /var/log/redis /etc/valkey /var/lib/valkey-* /srv/redis* /data/redis
  rm -f /root/dump.rdb /root/redis-*.log /root/appendonly*
  rm -rf /root/appendonlydir
  return 0
}

# The redis.io package at one version, which the package starts as `redis-server.service`.
install_package() {
  local version=$1 package
  package=$(apt-cache madison redis-server | awk -v v="$version" '$3 ~ "^6:"v"-" {print $3; exit}')
  [[ -n "$package" ]] || { log "redis.io has no $version"; return 1; }
  DEBIAN_FRONTEND=noninteractive apt-get -qqy install --allow-downgrades \
    "redis-server=$package" "redis-tools=$package" > /root/cells/install-redis.log 2>&1 || return 1
  wait_ready "redis-cli -p 6379"
  return $?
}

# Debian 12's own package, 7.0.15, from the Debian archive rather than redis.io's.
install_debian_package() {
  local package
  package=$(apt-cache madison redis-server | awk '$3 ~ /^5:7\.0\.15/ {print $3; exit}')
  DEBIAN_FRONTEND=noninteractive apt-get -qqy install --allow-downgrades \
    "redis-server=$package" "redis-tools=$package" > /root/cells/install-redis.log 2>&1 || return 1
  wait_ready "redis-cli -p 6379"
  return $?
}

# Polls until the command's `PING` gets any answer, `NOAUTH` included.
wait_ready() {
  local client=$1 answer
  for _ in $(seq 1 60); do
    answer=$($client PING 2>&1)
    if [[ "$answer" == PONG || "$answer" == *NOAUTH* || "$answer" == *WRONGPASS* ]]; then
      return 0
    fi
    sleep 1
  done
  log "never ready: $client"
  return 1
}

wait_container() {
  local name=$1 client=$2
  for _ in $(seq 1 60); do
    docker exec "$name" "$client" PING 2>/dev/null | grep -q PONG && return 0
    sleep 1
  done
  log "$name never answered"
  docker logs "$name" 2>&1 | tail -20
  return 1
}

restart_package() {
  systemctl restart redis-server
  wait_ready "redis-cli -p ${1:-6379}"
  return $?
}

set_directive() {
  local directive=$1 value=$2 file=${3:-$CONF}
  sed -i "/^$directive /d" "$file"
  echo "$directive $value" >> "$file"
  return 0
}

tls_certificates() {
  local directory=$1 owner=$2
  mkdir -p "$directory"
  openssl req -x509 -newkey rsa:2048 -nodes -days 30 -subj /CN=matrix \
    -keyout "$directory/server.key" -out "$directory/server.crt" >/dev/null 2>&1
  chown -R "$owner" "$directory"
  chmod 640 "$directory/server.key"
  return 0
}

# A unit of the operator's own for a server built from source, as valkey's docs suggest.
own_unit() {
  local name=$1 user=$2 start=$3 work=$4
  cat > "/etc/systemd/system/$name.service" <<UNIT
[Unit]
Description=$name, built from source
After=network.target

[Service]
Type=simple
User=$user
Group=$user
WorkingDirectory=$work
ExecStart=$start
Restart=always

[Install]
WantedBy=multi-user.target
UNIT
  systemctl daemon-reload
  systemctl start "$name"
  return 0
}

env_file() {
  local cell=$1
  cat > "/root/cells/$cell.env"
  return 0
}

package_env() {
  local cell=$1 extra=${2:-}
  env_file "$cell" <<ENV
REACH=tcp:6379
FILES="$CONF"
LOG=$PACKAGE_LOG
$extra
ENV
  return 0
}

cell_01() {
  install_package 8.10.2 || return 1
  package_env 01
  return 0
}

cell_02() {
  install_package 8.0.6 || return 1
  package_env 02
  return 0
}

cell_03() {
  # The field host and the estate, on redis 8: listening everywhere, a password in the file,
  # and memory settings changed at runtime.
  install_package 8.2.10 || return 1
  set_directive bind 0.0.0.0
  set_directive requirepass cell-three-password
  restart_package || return 1
  redis-cli -a cell-three-password --no-auth-warning CONFIG SET maxmemory 100mb >/dev/null
  redis-cli -a cell-three-password --no-auth-warning CONFIG SET save "" >/dev/null
  package_env 03 "PASSWORD=cell-three-password"
  return 0
}

cell_04() {
  install_package 8.4.7 || return 1
  redis-cli CONFIG SET requirepass cell-four-runtime-only >/dev/null
  package_env 04 "PASSWORD=cell-four-runtime-only"
  return 0
}

cell_05() {
  # `CONFIG REWRITE` keeps `requirepass` and appends the account line with its hash.
  install_package 8.6.7 || return 1
  redis-cli CONFIG SET requirepass cell-five-password >/dev/null
  redis-cli -a cell-five-password --no-auth-warning CONFIG REWRITE >/dev/null
  package_env 05 "PASSWORD=cell-five-password"
  return 0
}

cell_06() {
  # As 05, then `requirepass` edited by hand without a restart: the file states two passwords.
  install_package 8.6.7 || return 1
  redis-cli CONFIG SET requirepass cell-six-password >/dev/null
  redis-cli -a cell-six-password --no-auth-warning CONFIG REWRITE >/dev/null
  sed -i 's/^requirepass .*/requirepass cell-six-edited-by-hand/' "$CONF"
  package_env 06 "PASSWORD=cell-six-password"
  return 0
}

cell_07() {
  # Rotated at runtime after start: the file agrees with itself and the server disagrees.
  install_package 8.8.3 || return 1
  set_directive requirepass cell-seven-at-start
  restart_package || return 1
  redis-cli -a cell-seven-at-start --no-auth-warning CONFIG SET requirepass cell-seven-rotated >/dev/null
  package_env 07 "PASSWORD=cell-seven-rotated"
  return 0
}

cell_08() {
  install_package 8.8.3 || return 1
  cat > /etc/redis/users.acl <<'ACL'
user default on >cell-eight-default ~* &* +@all
user app on >cell-eight-app ~app:* +@read +@write
user monitor on >cell-eight-monitor ~* +info +ping
ACL
  chown redis:redis /etc/redis/users.acl
  chmod 640 /etc/redis/users.acl
  set_directive aclfile /etc/redis/users.acl
  restart_package || return 1
  package_env 08 "PASSWORD=cell-eight-default
FILES=\"$CONF /etc/redis/users.acl\""
  return 0
}

cell_09() {
  install_package 8.10.2 || return 1
  set_directive port 0
  set_directive unixsocket /run/redis/redis-server.sock
  set_directive unixsocketperm 770
  systemctl restart redis-server
  wait_ready "redis-cli -s /run/redis/redis-server.sock" || return 1
  package_env 09 "REACH=unix:/run/redis/redis-server.sock"
  return 0
}

cell_10() {
  install_package 8.10.2 || return 1
  tls_certificates /etc/redis/tls redis:redis
  set_directive port 6380
  set_directive tls-port 6379
  set_directive tls-cert-file /etc/redis/tls/server.crt
  set_directive tls-key-file /etc/redis/tls/server.key
  set_directive tls-auth-clients no
  systemctl restart redis-server
  wait_ready "redis-cli -p 6380" || return 1
  package_env 10 "REACH=tcp:6380"
  return 0
}

cell_11() {
  # TLS alone: the capture speaks no TLS, so the replies come from rastro's run only.
  install_package 8.10.2 || return 1
  tls_certificates /etc/redis/tls redis:redis
  set_directive port 0
  set_directive tls-port 6379
  set_directive tls-cert-file /etc/redis/tls/server.crt
  set_directive tls-key-file /etc/redis/tls/server.key
  set_directive tls-auth-clients no
  systemctl restart redis-server
  sleep 3
  package_env 11 "REACH="
  return 0
}

template_config() {
  local name=$1 port=$2
  sed -e "s/^port .*/port $port/" \
    -e "s#^pidfile .*#pidfile /run/redis-$name/redis-server.pid#" \
    -e "s#^logfile .*#logfile /var/log/redis/redis-server-$name.log#" \
    -e "s#^dir .*#dir /var/lib/redis/$name#" \
    "$CONF" > "/etc/redis/redis-$name.conf"
  mkdir -p "/var/lib/redis/$name"
  chown redis:redis "/etc/redis/redis-$name.conf" "/var/lib/redis/$name"
  chmod 640 "/etc/redis/redis-$name.conf"
  return 0
}

cell_12() {
  # Debian's template, two instances, one replicating the other.
  install_package 8.10.2 || return 1
  systemctl disable --now redis-server >/dev/null 2>&1
  template_config queue 6380
  template_config cache 6381
  echo "replicaof 127.0.0.1 6380" >> /etc/redis/redis-cache.conf
  systemctl start redis-server@queue redis-server@cache
  wait_ready "redis-cli -p 6380" || return 1
  wait_ready "redis-cli -p 6381" || return 1
  env_file 12 <<ENV
REACH_6380=tcp:6380
REACH_6381=tcp:6381
FILES="/etc/redis/redis-queue.conf /etc/redis/redis-cache.conf"
LOG_6380=/var/log/redis/redis-server-queue.log
LOG_6381=/var/log/redis/redis-server-cache.log
ENV
  return 0
}

cell_13() {
  install_package 8.10.2 || return 1
  set_directive cluster-enabled yes
  set_directive cluster-config-file nodes-6379.conf
  restart_package || return 1
  package_env 13
  return 0
}

cell_14() {
  install_package 8.10.2 || return 1
  echo 'rename-command CONFIG ""' >> "$CONF"
  restart_package || return 1
  package_env 14
  return 0
}

cell_15() {
  install_package 8.10.2 || return 1
  mkdir -p /etc/redis/conf.d
  echo "requirepass cell-fifteen-drop-in" > /etc/redis/conf.d/10-auth.conf
  chown -R redis:redis /etc/redis/conf.d
  set_directive set-proc-title no
  set_directive port 6390
  echo "include /etc/redis/conf.d/*.conf" >> "$CONF"
  systemctl restart redis-server
  wait_ready "redis-cli -p 6390" || return 1
  package_env 15 "REACH=tcp:6390
PASSWORD=cell-fifteen-drop-in
FILES=\"$CONF /etc/redis/conf.d/10-auth.conf\""
  return 0
}

cell_16() {
  install_package 8.10.2 || return 1
  mountpoint -q /data || return 1
  mkdir -p /data/redis
  chown redis:redis /data/redis
  mkdir -p /etc/systemd/system/redis-server.service.d
  printf '[Service]\nReadWriteDirectories=-/data/redis\n' > /etc/systemd/system/redis-server.service.d/data.conf
  systemctl daemon-reload
  set_directive dir /data/redis
  set_directive appendonly yes
  restart_package || return 1
  package_env 16
  return 0
}

cell_17() {
  # One module from the file, a second loaded at runtime.
  install_package 8.10.2 || return 1
  # Since 7.0, `enable-module-command no` refuses `MODULE LOAD` unless the operator allows it.
  sed -i '/^loadmodule .*redistimeseries/d' "$CONF"
  set_directive enable-module-command local
  restart_package || return 1
  redis-cli MODULE LOAD /usr/lib/redis/modules/redistimeseries.so | grep -qx OK || return 1
  package_env 17
  return 0
}

cell_18() {
  install_package 8.10.2 || return 1
  echo "user default off" >> "$CONF"
  echo "user app on >cell-eighteen-app ~* &* +@all" >> "$CONF"
  restart_package || return 1
  package_env 18 "PASSWORD=\"app cell-eighteen-app\""
  return 0
}

cell_19() {
  # Started by hand from root's home, with `dir ./`.
  install_package 8.10.2 || return 1
  systemctl disable --now redis-server >/dev/null 2>&1
  (cd /root && setsid redis-server --port 6379 --dir ./ --daemonize yes --logfile /root/redis-19.log)
  wait_ready "redis-cli -p 6379" || return 1
  env_file 19 <<'ENV'
REACH=tcp:6379
LOG=/root/redis-19.log
ENV
  return 0
}

cell_20() {
  install_package 8.10.2 || return 1
  systemctl disable --now redis-server >/dev/null 2>&1
  (cd / && setsid redis-server --port 6379 --requirepass cell-twenty-by-hand --daemonize yes \
    --logfile /root/redis-20.log)
  wait_ready "redis-cli -p 6379" || return 1
  env_file 20 <<'ENV'
REACH=tcp:6379
PASSWORD=cell-twenty-by-hand
LOG=/root/redis-20.log
ENV
  return 0
}

cell_21() {
  install_package 8.10.2 || return 1
  systemctl stop redis-server
  env_file 21 </dev/null
  return 0
}

cell_22() {
  install_package 8.10.2 || return 1
  systemctl disable --now redis-server >/dev/null 2>&1
  local package
  package=$(apt-cache madison redis-sentinel | awk '$3 ~ /^6:8\.10\.2-/ {print $3; exit}')
  DEBIAN_FRONTEND=noninteractive apt-get -qqy install "redis-sentinel=$package" >/dev/null 2>&1 || return 1
  wait_ready "redis-cli -p 26379" || return 1
  env_file 22 <<'ENV'
REACH=tcp:26379
FILES=/etc/redis/sentinel.conf
LOG=/var/log/redis/redis-sentinel.log
ENV
  return 0
}

cell_23() {
  docker run -d --name redis23 redis:8.10.2 >/dev/null || return 1
  wait_container redis23 redis-cli || return 1
  env_file 23 <<'ENV'
REACH=tcp:6379
LOG=docker:redis23
ENV
  return 0
}

cell_24() {
  docker run -d --name redis24 -p 6379:6379 -v redis24data:/data redis:8.10.2 >/dev/null || return 1
  wait_container redis24 redis-cli || return 1
  env_file 24 <<'ENV'
REACH=tcp:6379
LOG=docker:redis24
ENV
  return 0
}

cell_25() {
  mkdir -p /srv/redis25
  printf 'bind 0.0.0.0\nrequirepass cell-twenty-five-mounted\n' > /srv/redis25/redis.conf
  docker run -d --name redis25 -v /srv/redis25:/usr/local/etc/redis:ro redis:8.8.3 \
    redis-server /usr/local/etc/redis/redis.conf >/dev/null || return 1
  sleep 3
  env_file 25 <<'ENV'
REACH=tcp:6379
PASSWORD=cell-twenty-five-mounted
FILES=/usr/local/etc/redis/redis.conf
LOG=docker:redis25
ENV
  return 0
}

cell_26() {
  docker run -d --name valkey26 valkey/valkey:9.1.2 >/dev/null || return 1
  wait_container valkey26 valkey-cli || return 1
  env_file 26 <<'ENV'
REACH=tcp:6379
LOG=docker:valkey26
ENV
  return 0
}

valkey_config() {
  local cell=$1 body=$2
  mkdir -p /etc/valkey "/var/lib/valkey-$cell"
  printf '%b' "$body" > "/etc/valkey/valkey-$cell.conf"
  chown -R valkey:valkey /etc/valkey "/var/lib/valkey-$cell"
  chmod 640 "/etc/valkey/valkey-$cell.conf"
  return 0
}

cell_27() {
  valkey_config 27 "bind 0.0.0.0\nport 6379\ndir /var/lib/valkey-27\nlogfile /var/lib/valkey-27/valkey.log\nrequirepass cell-twenty-seven-password\n"
  own_unit valkey-27 valkey "/opt/valkey-9.0.6/bin/valkey-server /etc/valkey/valkey-27.conf" /var/lib/valkey-27
  wait_ready "/opt/valkey-9.0.6/bin/valkey-cli -p 6379" || return 1
  env_file 27 <<'ENV'
REACH=tcp:6379
PASSWORD=cell-twenty-seven-password
FILES=/etc/valkey/valkey-27.conf
LOG=/var/lib/valkey-27/valkey.log
ENV
  return 0
}

cell_28() {
  # Started through a `redis-server` link, as Debian's compatibility package installs one.
  ln -sf valkey-server /opt/valkey-8.1.10/bin/redis-server
  valkey_config 28 "bind 127.0.0.1 -::1\nport 6379\ndir /var/lib/valkey-28\nlogfile /var/lib/valkey-28/valkey.log\n"
  own_unit valkey-28 valkey "/opt/valkey-8.1.10/bin/redis-server /etc/valkey/valkey-28.conf" /var/lib/valkey-28
  wait_ready "/opt/valkey-8.1.10/bin/valkey-cli -p 6379" || return 1
  env_file 28 <<'ENV'
REACH=tcp:6379
FILES=/etc/valkey/valkey-28.conf
LOG=/var/lib/valkey-28/valkey.log
ENV
  return 0
}

cell_29() {
  mkdir -p /run/valkey-29
  chown valkey:valkey /run/valkey-29
  valkey_config 29 "port 0\nunixsocket /run/valkey-29/valkey.sock\nunixsocketperm 770\ndir /var/lib/valkey-29\nlogfile /var/lib/valkey-29/valkey.log\naclfile /etc/valkey/users-29.acl\n"
  printf 'user default on >cell-twenty-nine-default ~* &* +@all\nuser reader on >cell-twenty-nine-reader ~* +@read\n' > /etc/valkey/users-29.acl
  chown valkey:valkey /etc/valkey/users-29.acl
  own_unit valkey-29 valkey "/opt/valkey-8.0.11/bin/valkey-server /etc/valkey/valkey-29.conf" /var/lib/valkey-29
  wait_ready "/opt/valkey-8.0.11/bin/valkey-cli -s /run/valkey-29/valkey.sock" || return 1
  env_file 29 <<'ENV'
REACH=unix:/run/valkey-29/valkey.sock
PASSWORD=cell-twenty-nine-default
FILES="/etc/valkey/valkey-29.conf /etc/valkey/users-29.acl"
LOG=/var/lib/valkey-29/valkey.log
ENV
  return 0
}

cell_30() {
  tls_certificates /srv/redis30 999:999
  docker run -d --name valkey30 -v /srv/redis30:/tls:ro valkey/valkey:7.2.14 valkey-server \
    --port 6380 --tls-port 6379 --tls-cert-file /tls/server.crt --tls-key-file /tls/server.key \
    --tls-auth-clients no >/dev/null || return 1
  sleep 3
  env_file 30 <<'ENV'
REACH=tcp:6380
LOG=docker:valkey30
ENV
  return 0
}

cell_31() {
  install_package 8.10.2 || return 1
  docker run -d --name valkey31 valkey/valkey:9.1.2 >/dev/null || return 1
  wait_container valkey31 valkey-cli || return 1
  env_file 31 <<ENV
REACH=tcp:6379
FILES="$CONF"
ENV
  return 0
}

cell_32() {
  install_debian_package || return 1
  set_directive requirepass cell-thirty-two-password
  restart_package || return 1
  package_env 32 "PASSWORD=cell-thirty-two-password"
  return 0
}

cell_33() {
  install_package 7.4.11 || return 1
  package_env 33
  return 0
}

cell_34() {
  # The unit's command line outranks the file.
  install_package 6.2.24 || return 1
  set_directive requirepass cell-thirty-four-in-the-file
  mkdir -p /etc/systemd/system/redis-server.service.d
  printf '[Service]\nExecStart=\nExecStart=/usr/bin/redis-server /etc/redis/redis.conf --requirepass cell-thirty-four-command-line\n' \
    > /etc/systemd/system/redis-server.service.d/password.conf
  systemctl daemon-reload
  restart_package || return 1
  package_env 34 "PASSWORD=cell-thirty-four-command-line"
  return 0
}

cell_35() {
  # The field host's shape: redis 5 from source, its own unit, listening everywhere.
  mkdir -p /etc/redis /var/lib/redis5
  printf 'bind 0.0.0.0\nport 6379\ndir /var/lib/redis5\nlogfile /var/lib/redis5/redis.log\nrequirepass cell-thirty-five-password\n' \
    > /etc/redis/redis5.conf
  chown -R redis5:redis5 /etc/redis /var/lib/redis5
  own_unit redis5 redis5 "/opt/redis-$REDIS_5/bin/redis-server /etc/redis/redis5.conf" /var/lib/redis5
  wait_ready "/opt/redis-$REDIS_5/bin/redis-cli -p 6379" || return 1
  env_file 35 <<'ENV'
REACH=tcp:6379
PASSWORD=cell-thirty-five-password
FILES=/etc/redis/redis5.conf
LOG=/var/lib/redis5/redis.log
ENV
  return 0
}

# After the capture, so the logs hold what the capture caused.
collect_logs() {
  local cell=$1 out container file
  out=/captures/redis/$cell
  mkdir -p "$out/logs"
  for container in $(docker ps -a --format '{{.Names}}'); do
    docker logs "$container" > "$out/logs/docker-$container.log" 2>&1
  done
  journalctl -u 'redis*' -u 'valkey*' --since "-15min" --no-pager > "$out/logs/journal.log" 2>/dev/null
  for file in /var/log/redis/*.log /var/lib/valkey-*/valkey.log /var/lib/redis5/redis.log /root/redis-*.log; do
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
    mkdir -p "/captures/redis/$cell"
    echo "setup failed" > "/captures/redis/$cell/SETUP_FAILED"
  fi
  collect_logs "$cell"
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

#!/usr/bin/env bash
# Captures what rastro would see of every redis and valkey server on this box, for one matrix
# cell, then runs rastro over it as root and unprivileged and records what each run cost.
# Usage: capture.sh CELL   (reads /root/cells/CELL.env, see cells.sh: env_file)
set -u

readonly SERVER_NAMES='^(redis-server|valkey-server|redis-sentinel|valkey-sentinel)$'
readonly PROC_FILES=(cmdline comm stat status cgroup mountinfo net/tcp net/tcp6 net/unix)
readonly PROC_LINKS=(cwd root exe ns/net ns/mnt ns/pid)
readonly UNIT_PROPERTIES=(Id ExecStartEx ExecStart ControlGroup WorkingDirectory RootDirectory)
readonly COMMANDS=("INFO server" "CONFIG GET *" "INFO replication" "ACL LIST" "MODULE LIST")
readonly RASTRO=/usr/local/bin/rastro
readonly RASTRO_CONFIG=/etc/rastro-redis-only.toml

is_server() {
  local pid=$1 parent
  grep -qE "$SERVER_NAMES" "/proc/$pid/comm" 2>/dev/null || return 1
  # A background save keeps `comm` and closes the listeners: the parent is the server.
  parent=$(sed 's/.*) //' "/proc/$pid/stat" | cut -d' ' -f2)
  if grep -qE "$SERVER_NAMES" "/proc/$parent/comm" 2>/dev/null; then
    return 1
  fi
  return 0
}

# Copies a process's files and links, and notes which an unprivileged account may read.
copy_proc() {
  local pid=$1 dir=$2 file link readable descriptor
  mkdir -p "$dir"
  for file in "${PROC_FILES[@]}"; do
    mkdir -p "$dir/$(dirname "$file")"
    cat "/proc/$pid/$file" > "$dir/$file" 2>/dev/null || echo unreadable > "$dir/$file.error"
  done
  for link in "${PROC_LINKS[@]}"; do
    mkdir -p "$dir/$(dirname "$link")"
    readlink "/proc/$pid/$link" > "$dir/$link.link" 2>/dev/null || echo unreadable > "$dir/$link.error"
  done
  for descriptor in "/proc/$pid/fd"/*; do
    echo "${descriptor##*/} -> $(readlink "$descriptor")"
  done > "$dir/fd.list" 2>/dev/null
  : > "$dir/unprivileged.txt"
  for file in "${PROC_FILES[@]}" fd cwd root; do
    if runuser -u nobody -- sh -c "ls /proc/$pid/$file >/dev/null 2>&1 && cat /proc/$pid/$file >/dev/null 2>&1 || readlink /proc/$pid/$file >/dev/null 2>&1"; then
      readable=readable
    else
      readable=refused
    fi
    echo "$file $readable" >> "$dir/unprivileged.txt"
  done
  return 0
}

# What systemd says about the unit the server's cgroup names, where it names one.
copy_unit() {
  local pid=$1 dir=$2 unit property arguments=()
  unit=$(sed -n 's#^0::.*/\([^/]*\.service\)$#\1#p' "/proc/$pid/cgroup" | head -1)
  [[ -n "$unit" ]] || return 0
  echo "$unit" > "$dir/unit"
  for property in "${UNIT_PROPERTIES[@]}"; do arguments+=(--property="$property"); done
  systemctl show --no-pager "${arguments[@]}" -- "$unit" > "$dir/unit.show" 2>&1
  return 0
}

# The files the cell names, read inside the server's own root, at the same paths.
copy_files() {
  local pid=$1 dir=$2 path
  for path in ${FILES:-}; do
    mkdir -p "$dir/files$(dirname "$path")"
    cat "/proc/$pid/root$path" > "$dir/files$path" 2>"$dir/files$path.error" \
      && rm -f "$dir/files$path.error"
  done
  return 0
}

# The server's own address for the capture's client: inside its network namespace for TCP, and
# through its root for a unix socket, so a container is reached the same way as the host.
client_for() {
  local pid=$1 reach=$2
  case "$reach" in
    unix:*) echo "python3 /root/resp.py --unix /proc/$pid/root${reach#unix:}" ;;
    tcp:*) echo "nsenter -t $pid -n python3 /root/resp.py --tcp 127.0.0.1 ${reach#tcp:}" ;;
  esac
  return 0
}

# The reach a cell names for this server: `REACH_<port or name>` where several, `REACH` alone.
reach_of() {
  local server=$1 variable
  variable=REACH_$server
  echo "${!variable:-${REACH:-}}"
  return 0
}

password_file_of() {
  local server=$1 variable
  variable=PASSWORD_$server
  local password=${!variable:-${PASSWORD:-}}
  [[ -n "$password" ]] || return 0
  printf '%s\n' "$password" > "/root/cells/password-$server"
  echo "--password-file /root/cells/password-$server"
  return 0
}

ask() {
  local pid=$1 dir=$2 server=$3 reach client password
  reach=$(reach_of "$server")
  [[ -n "$reach" ]] || { echo "the cell names no reach for $server" > "$dir/replies.error"; return 0; }
  echo "$reach" > "$dir/reach"
  client=$(client_for "$pid" "$reach")
  password=$(password_file_of "$server")
  $client "$dir/replies/unauthenticated" "INFO server" 2> "$dir/replies.unauthenticated.error"
  # shellcheck disable=SC2086
  $client $password "$dir/replies/authenticated" "${COMMANDS[@]}" 2> "$dir/replies.error"
  # shellcheck disable=SC2086
  $client $password "$dir/cost/before" "ACL LOG" "ACL LOG RESET" 2> "$dir/cost.error"
  [[ -s "$dir/replies.error" ]] || rm -f "$dir/replies.error"
  [[ -s "$dir/replies.unauthenticated.error" ]] || rm -f "$dir/replies.unauthenticated.error"
  return 0
}

# The server's log, by line count, so what a rastro run added can be cut out afterwards.
log_lines() {
  local dir=$1 log
  log=$(cat "$dir/log-path" 2>/dev/null)
  case "$log" in
    docker:*) docker logs "${log#docker:}" 2>&1 | wc -l ;;
    /*) wc -l < "$log" 2>/dev/null || echo 0 ;;
    *) echo 0 ;;
  esac
  return 0
}

log_since() {
  local dir=$1 from=$2 log
  log=$(cat "$dir/log-path" 2>/dev/null)
  case "$log" in
    docker:*) docker logs "${log#docker:}" 2>&1 | tail -n +"$((from + 1))" ;;
    /*) tail -n +"$((from + 1))" "$log" 2>/dev/null ;;
  esac
  return 0
}

log_path_of() {
  local server=$1 variable
  variable=LOG_$server
  echo "${!variable:-${LOG:-}}"
  return 0
}

capture_server() {
  local pid=$1 dir=$2 server
  copy_proc "$pid" "$dir/process"
  copy_unit "$pid" "$dir"
  copy_files "$pid" "$dir"
  server=$(server_name_of "$pid")
  echo "$server" > "$dir/server"
  log_path_of "$server" > "$dir/log-path"
  ask "$pid" "$dir" "$server"
  return 0
}

# The name a cell's variables use for this server: its lowest listening TCP port, else `unix`.
server_name_of() {
  local pid=$1 inode hexport lowest=""
  while read -r inode; do
    while read -r hexport; do
      if [[ -z "$lowest" ]] || (( 16#$hexport < lowest )); then lowest=$((16#$hexport)); fi
    done < <(awk -v inode="$inode" '$4 == "0A" && $10 == inode { n = split($2, a, ":"); print a[n] }' \
      "/proc/$pid/net/tcp" "/proc/$pid/net/tcp6")
  done < <(for descriptor in "/proc/$pid/fd"/*; do readlink "$descriptor"; done 2>/dev/null \
    | sed -n 's/^socket:\[\([0-9]*\)\]$/\1/p')
  echo "${lowest:-unix}"
  return 0
}

run_rastro() {
  local out=$1
  "$RASTRO" --config "$RASTRO_CONFIG" --no-progress -o - > "$out/facet-root.json" 2> "$out/stderr-root.txt"
  echo $? > "$out/exit-root"
  runuser -u nobody -- "$RASTRO" --config "$RASTRO_CONFIG" --no-progress -o - \
    > "$out/facet-unprivileged.json" 2> "$out/stderr-unprivileged.txt"
  echo $? > "$out/exit-unprivileged"
  return 0
}

main() {
  local cell=$1 out process found=0 dir
  local -A lines
  out=/captures/redis/$cell
  rm -rf "$out"
  mkdir -p "$out"
  REACH=""
  PASSWORD=""
  FILES=""
  LOG=""
  # shellcheck source=/dev/null
  [[ -f "/root/cells/$cell.env" ]] && . "/root/cells/$cell.env"

  { echo "cell $cell"; date -u +%FT%TZ; uname -a; } > "$out/meta.txt"
  readlink /proc/self/ns/net > "$out/host-netns.link"
  readlink /proc/self/ns/mnt > "$out/host-mntns.link"
  # rastro reads the socket tables of its own namespace, which a container's server is not in.
  mkdir -p "$out/host-net"
  for table in tcp tcp6 unix; do cat "/proc/self/net/$table" > "$out/host-net/$table"; done
  if command -v docker >/dev/null; then
    docker ps --format '{{.ID}} {{.Names}} {{.Image}}' > "$out/containers.txt" 2>/dev/null
  fi

  for process in /proc/[0-9]*; do
    is_server "${process#/proc/}" || continue
    found=$((found + 1))
    dir=$out/server-$found
    capture_server "${process#/proc/}" "$dir"
    lines[$dir]=$(log_lines "$dir")
  done
  echo "$found" > "$out/server-count"

  run_rastro "$out"

  for dir in "$out"/server-[0-9]*; do
    [[ -d "$dir" ]] || continue
    local server client password reach pid
    server=$(cat "$dir/server")
    reach=$(cat "$dir/reach" 2>/dev/null) || continue
    pid=$(cut -d' ' -f1 "$dir/process/stat")
    client=$(client_for "$pid" "$reach")
    password=$(password_file_of "$server")
    # shellcheck disable=SC2086
    $client $password "$dir/cost/after" "ACL LOG" 2>> "$dir/cost.error"
    log_since "$dir" "${lines[$dir]}" > "$dir/cost/log-during-rastro.txt"
    [[ -s "$dir/cost.error" ]] || rm -f "$dir/cost.error"
  done
  rm -f /root/cells/password-*
  echo "captured $found server(s) into $out"
  return 0
}

main "$@"

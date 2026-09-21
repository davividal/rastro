#!/bin/sh
# What reading a RabbitMQ node costs the box it is read from.
#
# The measurement behind the `rabbitmq` entries in docs/decisions.md, kept so the claims can
# be re-run rather than believed. Meant to be executed *inside* a throwaway container:
#
#   podman run --rm -v "$PWD/scripts":/s debian:trixie sh /s/rabbitmq-footprint.sh
#
# It installs rabbitmq-server, starts and stops a broker, and writes files. Do not run it on
# a box you care about.
#
# The five questions, in the order the facet's design depends on them:
#
#   1. Does invoking a CLI tool start epmd when none is running, and does it survive?
#   2. Does an invocation create an Erlang cookie in the caller's home?
#   3. Does a read append to the broker's log, or otherwise touch the box?
#   4. What does an invocation leave in /tmp and the data directory?
#   5. What does one invocation cost, since each is an Erlang VM boot?
#
# Question 3 needs a control window: an idle broker writes to its own log and its own store,
# so a file that moved during a read means nothing until the same window has been watched
# with no read in it.
set -u

OUT=${OUT:-/tmp/rabbitmq-footprint}
mkdir -p "$OUT"
REPORT="$OUT/report.txt"
: > "$REPORT"

say() { printf '%s\n' "$*" | tee -a "$REPORT"; }
section() { say ""; say "=== $*"; }

# Files touched since a marker, kernel interfaces and this script's own output aside.
touched_since() {
    find / -xdev -newer "$1" \
        -not -path '/proc/*' -not -path '/sys/*' -not -path "$OUT/*" \
        -not -path '/run/*' 2>/dev/null | sort
}

# One invocation, timed, with its streams kept. Never fails the run: a command that refuses
# is itself a measurement.
run() {
    name=$1
    shift
    start=$(date +%s%N)
    "$@" > "$OUT/$name.out" 2> "$OUT/$name.err"
    status=$?
    end=$(date +%s%N)
    say "$(printf '%-34s exit=%-3s %6s ms %10s bytes' \
        "$name" "$status" "$(( (end - start) / 1000000 ))" "$(wc -c < "$OUT/$name.out")")"
}

epmd_state() {
    if pgrep -a epmd > /dev/null 2>&1; then
        say "  epmd: RUNNING -> $(pgrep -a epmd | tr '\n' ';')"
    else
        say "  epmd: not running"
    fi
}

cookie_state() {
    for cookie in /root/.erlang.cookie /var/lib/rabbitmq/.erlang.cookie; do
        if [ -e "$cookie" ]; then
            say "  cookie $cookie: $(stat -c '%A %U:%G %s bytes' "$cookie")"
        else
            say "  cookie $cookie: absent"
        fi
    done
}

section "the box"
. /etc/os-release
say "$PRETTY_NAME, $(uname -srm)"

section "installing rabbitmq-server, not starting it"
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq > /dev/null 2>&1
apt-get install -y -qq --no-install-recommends rabbitmq-server procps > "$OUT/install.log" 2>&1
say "rabbitmq-server: $(dpkg-query -W -f '${Version}' rabbitmq-server 2>/dev/null)"
say "erlang-base:     $(dpkg-query -W -f '${Version}' erlang-base 2>/dev/null)"
cp /proc/modules "$OUT/modules.before"

section "baseline: nothing should be up"
if pgrep -f beam.smp > /dev/null 2>&1; then
    pkill -f beam.smp; sleep 3; pkill epmd 2>/dev/null; sleep 1
    say "  the install started a broker; stopped it to ask question 1 honestly"
fi
epmd_state
cookie_state

section "Q1a: does 'epmd -names' start epmd"
run epmd_names epmd -names
sed 's/^/  /' "$OUT/epmd_names.out" "$OUT/epmd_names.err" | tee -a "$REPORT" > /dev/null
epmd_state
pkill epmd 2>/dev/null && say "  (killed an epmd this probe left behind)"
sleep 1

section "Q1b/Q2/Q4: 'rabbitmqctl status' with the node down, as root then as rabbitmq"
for caller in root rabbitmq; do
    touch "/tmp/.marker-$caller"
    sleep 1
    case $caller in
        root) run "ctl_status_$caller" rabbitmqctl status ;;
        *) start=$(date +%s%N)
           su -s /bin/sh "$caller" -c 'HOME=/var/lib/rabbitmq rabbitmqctl status' \
               > "$OUT/ctl_status_$caller.out" 2> "$OUT/ctl_status_$caller.err"
           say "$(printf '%-34s exit=%-3s %6s ms' "ctl_status_$caller" "$?" \
               "$(( ($(date +%s%N) - start) / 1000000 ))")" ;;
    esac
    epmd_state
    cookie_state
    say "  files touched since the marker:"
    touched_since "/tmp/.marker-$caller" | sed 's/^/    /' | tee -a "$REPORT"
    pkill epmd 2>/dev/null && say "  (killed an epmd this invocation left behind)"
    sleep 1
done

section "starting the broker, so the reads can be asked of a live node"
su -s /bin/sh rabbitmq -c 'HOME=/var/lib/rabbitmq rabbitmq-server -detached' > "$OUT/start.log" 2>&1
waited=0
while [ "$waited" -lt 60 ]; do
    rabbitmqctl await_startup > /dev/null 2>&1 && break
    sleep 2
    waited=$(( waited + 2 ))
done
say "  node answered after about ${waited}s"
epmd_state
cookie_state
LOG=$(rabbitmqctl eval 'rabbit:log_locations().' 2>/dev/null | tr -d '[]"' | cut -d, -f1)
say "  log location: ${LOG:-unknown}"

section "seeding the entries an empty broker has none of"
cat > /tmp/seed.json <<'JSON'
{"vhosts":[{"name":"spike"}],
 "users":[{"name":"spikeuser","password":"hunter2","tags":["monitoring"]}],
 "permissions":[{"user":"spikeuser","vhost":"spike","configure":"^spike.*","write":".*","read":".*"}],
 "topic_permissions":[{"user":"spikeuser","vhost":"spike","exchange":"amq.topic","write":"^a","read":"^b"}],
 "policies":[{"vhost":"spike","name":"ha","pattern":"^work","apply-to":"queues","definition":{"max-length":1000},"priority":1}],
 "queues":[{"name":"work","vhost":"spike","durable":true,"auto_delete":false,"arguments":{"x-queue-type":"quorum"}}],
 "exchanges":[{"name":"spike.direct","vhost":"spike","type":"direct","durable":true,"auto_delete":false,"arguments":{}}],
 "bindings":[{"source":"spike.direct","vhost":"spike","destination":"work","destination_type":"queue","routing_key":"k","arguments":{}}]}
JSON
run import_definitions rabbitmqctl import_definitions /tmp/seed.json
rabbitmq-plugins enable rabbitmq_shovel rabbitmq_federation > /dev/null 2>&1
# A shovel and a federation upstream keep a peer's password inside a URI, which is the
# measurement behind withholding every parameter value.
rabbitmqctl set_parameter shovel my-shovel \
  '{"src-protocol":"amqp091","src-uri":"amqp://shovel-user:hunter2@upstream.example.com","src-queue":"in","dest-protocol":"amqp091","dest-uri":"amqp://localhost","dest-queue":"out"}' > /dev/null 2>&1
rabbitmqctl set_parameter federation-upstream my-upstream \
  '{"uri":"amqp://fed-user:s3cret@peer.example.com","expires":3600000}' > /dev/null 2>&1
rabbitmqctl set_operator_policy capped '^work' '{"max-length":5000}' --apply-to queues > /dev/null 2>&1
rabbitmqctl set_global_parameter my-global '{"answer":42,"fraction":0.5,"on":true}' > /dev/null 2>&1

section "Q3 control: what an idle broker touches on its own in 10s"
touch /tmp/.marker-idle
sleep 10
IDLE_LOG_DIGEST=$( [ -f "$LOG" ] && md5sum "$LOG" | cut -d' ' -f1 )
touched_since /tmp/.marker-idle | sed 's/^/    /' | tee -a "$REPORT"
say "  log digest after the idle window: ${IDLE_LOG_DIGEST:-no log file}"

section "Q3/Q5: the reads, timed, and what they touched"
touch /tmp/.marker-reads
sleep 1
run status                 rabbitmqctl status --formatter json
run cluster_status         rabbitmqctl cluster_status --formatter json
run environment            rabbitmqctl environment --formatter json
run export_definitions     rabbitmqctl export_definitions -
run list_feature_flags     rabbitmqctl list_feature_flags --formatter json
run diag_listeners         rabbitmq-diagnostics listeners --formatter json
run plugins_list           rabbitmq-plugins list --formatter json
say ""
say "  files touched during the read window:"
touched_since /tmp/.marker-reads | sed 's/^/    /' | tee -a "$REPORT"
say "  log digest after the reads:       $( [ -f "$LOG" ] && md5sum "$LOG" | cut -d' ' -f1 )"
say "  log digest after the idle window: ${IDLE_LOG_DIGEST:-no log file}"

section "the register, with epmd up, and whether a read leaves its own node in it"
epmd -names | sed 's/^/  /' | tee -a "$REPORT"
epmd -names > /tmp/names.before 2>&1
rabbitmqctl list_users > /dev/null 2>&1
sleep 1
epmd -names > /tmp/names.after 2>&1
if diff -q /tmp/names.before /tmp/names.after > /dev/null 2>&1; then
    say "  identical before and after a CLI call: the tool deregisters"
else
    say "  CHANGED, which would mean a read leaves a node registered:"
    diff -u /tmp/names.before /tmp/names.after | sed 's/^/    /' | tee -a "$REPORT"
fi

section "can a node be attributed to a process, and what stops it"
say "  capabilities of this shell: $(grep CapEff /proc/self/status)"
PID=$(pgrep beam.smp | head -1)
say "  beam pid $PID owned by $(stat -c %U "/proc/$PID" 2>&1)"
say "  its descriptors readable: $(ls "/proc/$PID/fd" > /dev/null 2>&1 && echo yes || echo 'NO, which is what a reduced-capability container gives even as root')"
say "  its environ variable count: $(tr '\0' '\n' < "/proc/$PID/environ" | grep -c .)"
say "  the boot call in its argv: $(tr '\0' ' ' < "/proc/$PID/cmdline" | grep -o '\-s rabbit boot' || echo absent)"

section "the data directory, for the sealing argument"
DATA=$(rabbitmqctl eval 'rabbit_mnesia:dir().' 2>/dev/null | tr -d '"')
say "  data directory: ${DATA:-unknown}"
[ -d "$DATA" ] && say "  entries: $(find "$DATA" | wc -l), size: $(du -sk "$DATA" | cut -f1) KiB"

section "kernel modules, before and after everything"
cp /proc/modules "$OUT/modules.after"
if diff -q "$OUT/modules.before" "$OUT/modules.after" > /dev/null 2>&1; then
    say "  unchanged"
else
    say "  CHANGED (a container shares the host kernel, so read it with that in mind):"
    diff -u "$OUT/modules.before" "$OUT/modules.after" | sed 's/^/    /' | tee -a "$REPORT"
fi

section "done"
say "report and captured answers in $OUT"

#!/usr/bin/env bash
for n in $(seq -w 1 26); do /root/cells.sh run "$n"; done
/root/cells.sh reset
echo "ALL DONE"

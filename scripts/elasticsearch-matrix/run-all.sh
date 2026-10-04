#!/usr/bin/env bash
# Every cell of the matrix, one after another, then a clean box.
set -u
for cell in $(seq -w 1 26); do
  /root/cells.sh run "$cell"
done
/root/cells.sh reset
echo "ALL DONE"

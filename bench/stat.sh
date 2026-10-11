#!/bin/bash
# usage: stat.sh <bin> [label] [reps]
# User-space instruction count and best-of-N CPU time for one process per PDF,
# first-2-pages and full-text passes, over $PDFBENCH/s (default /tmp/pdfbench/s).
b=$1; l=${2:-$(basename "$b")}; reps=${3:-3}
here=$(dirname "$0"); src="${PDFBENCH:-/tmp/pdfbench}/s"; n=$(ls "$src"/*.pdf | wc -l)
for m in "-l 2" ""; do
  best=""
  for r in $(seq "$reps"); do
    line=$(perf stat -x, -e instructions:u,task-clock "$here/loop.sh" "$b" "$src" $m -layout 2>&1 \
      | awk -F, '/instructions/ {i=$1} /task-clock/ {t=$1} END {printf "%.0f %.2f", t, i/1e9}')
    if [ -z "$best" ] || [ "${line%% *}" -lt "${best%% *}" ]; then best="$line"; fi
  done
  echo "$best" | awk -v l="$l" -v m="${m:-full}" -v n="$n" '{printf "%-12s %-5s %7.2f G instr  cpu %5.2fs  %5.1f ms/file\n", l, m, $2, $1/1000, $1/n}'
done

#!/bin/bash
# usage: loop.sh <bin> <srcdir> [extra args]  -- runs bin over every pdf, output discarded
b=$1; d=$2; shift 2
for f in "$d"/*.pdf; do "$b" "$@" "$f" - >/dev/null 2>&1; done

#!/usr/bin/env bash
# Usage: keylog.sh NAME — what the suite's windows run: each key typed into the
# window goes to ~/out.txt as "key NAME CHAR".
stty -icanon -echo
while IFS= read -r -n1 -d '' c; do
  printf 'key %s %s\n' "$1" "$c" >> ~/out.txt
done

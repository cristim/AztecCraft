#!/usr/bin/env bash
# maya-catalog-crawl.sh: regenerate the Autodesk Maya catalog name lists locally.
#
# Usage: scripts/maya-catalog-crawl.sh [OUTPUT_DIR]
#        scripts/maya-catalog-crawl.sh --help
#        AZTEC_CRAWL_SELFTEST=1 scripts/maya-catalog-crawl.sh   (offline extractor test)
# Default OUTPUT_DIR: <repo root>/target/maya-catalog, independent of the cwd.
#
# Fetches 8 public index pages from help.autodesk.com (4 per release, 2027 and
# 2026) one after another, and extracts only each row's NAME cell (never the
# description column):
#   commands    Maya-Tech-Docs/Commands/index_all.html  (linked command names)
#   nodes       Maya-Tech-Docs/Nodes/index_all.html     (linked node names)
#   cpp-classes MAYA-API-REF/cpp_ref/annotated.html     (Doxygen rows, names
#               qualified with their ancestors, e.g. Ns::Class)
#   py2-classes MAYA-API-REF/py_ref/annotated.html      (Doxygen rows, bare names)
# Output files: commands.txt nodes.txt cpp-classes.txt py2-classes.txt and the
# same four with a -2026 suffix. Each has a header ("# source:", "# retrieved:",
# "# count:") then one name per line in first-seen page order, de-duplicated.
# It then prints the 2026-to-2027 delta (names only in one release) per catalog
# and the counts, so the committed delta table can be re-derived.
#
# Output is for LOCAL USE ONLY and must never be committed: the lists are a bulk
# copy of Autodesk's catalog structure (owner decision on PR #457; the repo keeps
# only the delta and counts). Guard: inside a git work tree the script refuses
# unless git ignores all 8 output paths; outside any work tree only an explicit
# OUTPUT_DIR is accepted. All 8 files are built in a temp dir and moved into
# OUTPUT_DIR only after every page passes verification.
#
# Structure check: each count must be within 10 percent of the reference count
# below (observed 2026-11-15, used for both releases) and every name must match
# [A-Za-z0-9_:.]+, or the script fails: a page layout change would otherwise
# silently produce a bad list. Raise the references after legitimate growth.
# Fetch: https only, 3 attempts per page with a growing delay (5s, 10s), 2s
# pause between pages.
#
# Known differences from the PowerShell original (PR #457): no NBSP trimming;
# only the entities &lt; &gt; &quot; &#39; &amp; are decoded (any other entity
# leaves a character outside the allowed set and fails loudly).
# Needs only bash (3.2 ok), curl, awk, sed, grep, sort, comm, git (optional).
set -euo pipefail
export LC_ALL=C

BASE="https://help.autodesk.com/cloudhelp"
UA="AztecCraft-maya-catalog-crawl/1 (https://github.com/cristim/AztecCraft; one-shot polite fetch of 8 public index pages for local comparison)"

# name|path|mode|reference count
CATALOGS="commands|Maya-Tech-Docs/Commands/index_all.html|links|1528
nodes|Maya-Tech-Docs/Nodes/index_all.html|links|944
cpp-classes|MAYA-API-REF/cpp_ref/annotated.html|qualified|640
py2-classes|MAYA-API-REF/py_ref/annotated.html|bare|313"

decode() { sed -e 's/&lt;/</g' -e 's/&gt;/>/g' -e 's/&quot;/"/g' -e 's/&#39;/'"'"'/g' -e 's/&amp;/\&/g'; }

# Print names from a page on stdout, first-seen order, de-duplicated.
extract() { # file mode
  local file="$1" mode="$2"
  if [ "$mode" = links ]; then
    { tr '\n\r' '  ' <"$file" | { grep -oE 'href="[^"]+\.html"[^>]*>[^<]+</a>' || true; }; } |
      sed -E 's/^[^>]*>//; s/<\/a>$//' | decode |
      sed -E 's/^[[:space:]]+//; s/[[:space:]]+$//' | awk 'NF && !s[$0]++'
  else
    tr '\n\r' '  ' <"$file" | sed 's#</tr>#</tr>\
#g' | awk -v mode="$mode" '
      function trim(s) { gsub(/^[ \t]+|[ \t]+$/, "", s); return s }
      {
        if (!match($0, /<tr id="row_[0-9_]+"/)) next
        id = substr($0, RSTART + 8, RLENGTH - 9)
        if (!match($0, /<td class="entry">/)) next
        cell = substr($0, RSTART + RLENGTH)
        e = index(cell, "</td>"); if (e) cell = substr(cell, 1, e - 1)
        if (match(cell, /<a class="el"[^>]*>[^<]+<\/a>/)) {
          sym = substr(cell, RSTART, RLENGTH); sub(/^[^>]*>/, "", sym); sub(/<\/a>$/, "", sym)
        } else if (match(cell, /<b>[^<]+<\/b>/)) {
          sym = substr(cell, RSTART + 3, RLENGTH - 7)
        } else { print "warning: row " id " has no name cell" > "/dev/stderr"; next }
        sym = trim(sym); if (sym == "") next
        symof[id] = sym
        out = sym
        if (mode == "qualified") {
          n = split(substr(id, 5), seg, "_"); pre = "row_"; out = ""
          for (i = 1; i <= n; i++) {
            if (seg[i] == "") continue
            pre = pre seg[i] "_"
            if (pre in symof) out = out (out == "" ? "" : "::") symof[pre]
          }
        }
        print out
      }' | decode | awk 'NF && !s[$0]++'
  fi
}

selftest() {
  local d; d="$(mktemp -d "${TMPDIR:-/tmp}/maya-crawl-test.XXXXXX")"
  cat >"$d/in.html" <<'HTML'
<table>
<tr id="row_0_" class="even"><td class="entry"><span class="icon">N</span><b>Ns</b></td><td class="desc"></td></tr>
<tr id="row_0_0_" style="display:none;"><td class="entry"><a class="el" href="a.html">Alpha</a></td><td class="desc">See <a class="el" href="x.html">DescLink</a></td></tr>
<tr id="row_0_1_"><td class="entry"><a class="el" href="b.html">Beta &amp; Co</a></td><td class="desc"></td></tr>
<tr id="row_1_"><td class="entry"><a class="el" href="c.html">Alpha</a></td><td class="desc"></td></tr>
<tr id="row_1_0_"><td class="entry"><a class="el" href="d.html">Alpha</a></td><td class="desc"></td></tr>
</table>
HTML
  printf '%s\n' Ns Ns::Alpha 'Ns::Beta & Co' Alpha 'Alpha::Alpha' >"$d/want"
  extract "$d/in.html" qualified >"$d/got"
  printf '%s\n' Ns Alpha 'Beta & Co' >"$d/want-bare"
  extract "$d/in.html" bare >"$d/got-bare"
  local rc=0
  diff "$d/want" "$d/got" || rc=1
  diff "$d/want-bare" "$d/got-bare" || rc=1
  rm -rf "$d"
  [ "$rc" -eq 0 ] && echo "selftest ok"
  return "$rc"
}

usage() { echo "usage: $0 [OUTPUT_DIR] | --help" >&2; exit 2; }

if [ "${AZTEC_CRAWL_SELFTEST:-}" = 1 ]; then selftest; exit; fi

case "${1:-}" in
  -h|--help) sed -n '2,/^set -euo/p' "$0" | sed '$d;s/^# \{0,1\}//'; exit 0 ;;
  -*) echo "error: unknown option $1" >&2; usage ;;
esac
[ "$#" -le 1 ] || { echo "error: too many arguments" >&2; usage; }

if [ "$#" -eq 1 ] && [ -n "$1" ]; then
  OUT="$1"
else
  root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel 2>/dev/null)" || {
    echo "error: not inside a git work tree; pass an explicit OUTPUT_DIR" >&2; exit 2; }
  OUT="$root/target/maya-catalog"
fi

NAMES="commands commands-2026 nodes nodes-2026 cpp-classes cpp-classes-2026 py2-classes py2-classes-2026"
created=""; d="$OUT"
while [ ! -d "$d" ]; do created="$created
$d"; d="$(dirname "$d")"; done
mkdir -p "$OUT"
if git -C "$OUT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  for n in $NAMES; do
    if ! git -C "$OUT" check-ignore -q "$n.txt"; then
      echo "error: $OUT/$n.txt is not git-ignored; refusing to write catalog lists inside a work tree (they must not be committed)" >&2
      # remove only the directories this run created, deepest first, as collected (rmdir fails on non-empty)
      printf '%s\n' "$created" | while IFS= read -r c; do [ -z "$c" ] || rmdir "$c" 2>/dev/null || true; done
      exit 1
    fi
  done
fi

TMP="$(mktemp -d "${TMPDIR:-/tmp}/maya-crawl.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT
mkdir "$TMP/out"

fetch() { # url dest
  local attempt=1
  while ! curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' \
      --connect-timeout 20 --max-time 120 --user-agent "$UA" --output "$2" "$1"; do
    [ "$attempt" -lt 3 ] || { echo "error: giving up on $1" >&2; return 1; }
    sleep $((attempt * 5)); attempt=$((attempt + 1))
  done
}

NOW="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
firstfetch=1
for rel in 2027 2026; do
  suffix=""; [ "$rel" = 2026 ] && suffix="-2026"
  while IFS="|" read -r name path mode ref; do
    [ "$firstfetch" = 1 ] || sleep 2
    firstfetch=0
    url="$BASE/$rel/ENU/$path"
    fetch "$url" "$TMP/$name$suffix.html"
    extract "$TMP/$name$suffix.html" "$mode" >"$TMP/$name$suffix.names"
    count="$(wc -l <"$TMP/$name$suffix.names" | tr -d ' ')"
    lo=$((ref * 90 / 100)); hi=$((ref * 110 / 100))
    if [ "$count" -eq 0 ] || [ "$count" -lt "$lo" ] || [ "$count" -gt "$hi" ]; then
      echo "error: $url yielded $count names, expected $lo..$hi (reference $ref); the page structure probably changed" >&2
      exit 1
    fi
    if grep -qE '[^A-Za-z0-9_:.]' "$TMP/$name$suffix.names"; then
      echo "error: $url produced a name with unexpected characters; the page structure probably changed" >&2
      exit 1
    fi
    { printf '# source: %s\n# retrieved: %s\n# count: %s\n' "$url" "$NOW" "$count"; cat "$TMP/$name$suffix.names"; } >"$TMP/out/$name$suffix.txt"
    echo "$rel $name: $count names"
  done <<<"$CATALOGS"
done

for n in $NAMES; do mv -f "$TMP/out/$n.txt" "$OUT/$n.txt"; done
echo "wrote 8 lists to $OUT"

echo
echo "== 2026 -> 2027 delta =="
while IFS="|" read -r name _ _ _; do
  sort "$TMP/$name.names" >"$TMP/new.sorted"
  sort "$TMP/$name-2026.names" >"$TMP/old.sorted"
  added="$(comm -23 "$TMP/new.sorted" "$TMP/old.sorted")"
  removed="$(comm -13 "$TMP/new.sorted" "$TMP/old.sorted")"
  echo "-- $name: 2026 $(wc -l <"$TMP/$name-2026.names" | tr -d ' '), 2027 $(wc -l <"$TMP/$name.names" | tr -d ' ')"
  echo "only in 2027:"; [ -z "$added" ] || printf "%s\n" "$added" | sed "s/^/  /"
  echo "only in 2026:"; [ -z "$removed" ] || printf "%s\n" "$removed" | sed "s/^/  /"
done <<<"$CATALOGS"

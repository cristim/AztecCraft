#!/usr/bin/env bash
# maya-catalog-crawl.sh: regenerate the Autodesk Maya catalog name lists locally.
#
# Usage: scripts/maya-catalog-crawl.sh [OUTPUT_DIR]
#        scripts/maya-catalog-crawl.sh --help
#        AZTEC_CRAWL_SELFTEST=1 scripts/maya-catalog-crawl.sh   (offline extractor test)
# Default OUTPUT_DIR: <repo root>/target/maya-catalog, independent of the cwd.
#
# Fetches exactly 8 public year-level index pages from help.autodesk.com (4 each
# for 2027 and 2026). These are mutable year pages, not immutable update catalogs.
# Extracts only each row's NAME cell (never the
# description column):
#   commands    Maya-Tech-Docs/Commands/index_all.html  (linked command names)
#   nodes       Maya-Tech-Docs/Nodes/index_all.html     (linked node names)
#   cpp-classes MAYA-API-REF/cpp_ref/annotated.html     (Doxygen rows, names
#               qualified with their ancestors, e.g. Ns::Class)
#   py2-classes MAYA-API-REF/py_ref/annotated.html      (Doxygen rows, bare names)
# Each unique snapshot contains raw pages, extracted .names lists, and a manifest
# recording retrieval time, URLs, SHA-256 hashes, counts, and validation status.
#
# Output is for LOCAL USE ONLY and must never be committed: the lists are a bulk
# copy of Autodesk's catalog structure (owner decision on PR #457; the repo keeps
# only the delta and counts). Guard: inside a git work tree the script refuses
# unless git ignores the output directory; outside any work tree only an explicit
# OUTPUT_DIR is accepted. Each run is staged under OUTPUT_DIR and atomically
# published as a unique snapshot only after all pages validate; prior snapshots
# are never overwritten.
#
# The ±10% count references below (observed 2026-11-15, used for both releases)
# are structural sanity tripwires only, not completeness evidence. Names must
# also match [A-Za-z0-9_:.]+. These year-level pages are mutable.
# Fetch: https only, 3 attempts per page with a growing delay (5s, 10s), 2s
# pause between pages.
#
# Known differences from the PowerShell original (PR #457): no NBSP trimming;
# only the entities &lt; &gt; &quot; &#39; &amp; are decoded (any other entity
# leaves a character outside the allowed set and fails loudly).
# Needs bash (3.2 ok), curl, awk, sed, grep, sort, comm, and sha256sum or
# shasum; git is optional.
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

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    echo "error: need sha256sum or shasum" >&2
    return 1
  fi
}

validate_names() { # names-file reference-count
  local file="$1" ref="$2" count lo hi
  count="$(wc -l <"$file" | tr -d ' ')"
  lo=$((ref * 90 / 100)); hi=$((ref * 110 / 100))
  if [ "$count" -eq 0 ] || [ "$count" -lt "$lo" ] || [ "$count" -gt "$hi" ]; then
    echo "error: $file yielded $count names, expected $lo..$hi (reference $ref); the page structure probably changed" >&2
    return 1
  fi
  if grep -qE '[^A-Za-z0-9_:.]' "$file"; then
    echo "error: $file contains a name with unexpected characters; the page structure probably changed" >&2
    return 1
  fi
}

publish_snapshot() { # staged-dir output-dir filesystem-safe-stamp
  local staged="$1" output="$2" stamp="$3" destination
  destination="$output/snapshot-$stamp-$$"
  [ ! -e "$destination" ] || { echo "error: snapshot destination already exists: $destination" >&2; return 1; }
  mv "$staged" "$destination"
}

cleanup_created() {
  printf '%s\n' "$created" | while IFS= read -r c; do
    [ -z "$c" ] || rmdir "$c" 2>/dev/null || true
  done
}

guard_output() { # output-dir, relies on $created
  local output="$1" root rel d absolute
  if ! absolute="$(cd "$output" 2>/dev/null && pwd -P)"; then
    echo "error: cannot resolve output directory path: $output" >&2
    return 1
  fi
  if root="$(git -C "$output" rev-parse --show-toplevel 2>/dev/null)"; then
    rel="$(git -C "$output" rev-parse --show-prefix 2>/dev/null)"
    if ! git -C "$root" check-ignore -q "${rel:-.}"; then
      echo "error: $output is not git-ignored; refusing to write local catalog snapshots" >&2
      return 1
    fi
    return 0
  fi
  # A failed git probe is not evidence that this is outside a worktree.
  d="$absolute"
  while [ -n "$d" ]; do
    if [ -e "$d/.git" ]; then
      echo "error: git metadata exists at $d/.git but git cannot resolve it; refusing to write snapshots" >&2
      return 1
    fi
    [ "$d" != "/" ] || break
    d="$(dirname "$d")"
  done
  return 0
}

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
  # Duplicate names are intentionally de-duplicated; malformed/empty/
  # truncated structures must fail structural validation.
  printf '%s\n' Alpha Alpha >"$d/duplicate"
  [ "$(awk '!s[$0]++' "$d/duplicate" | wc -l | tr -d ' ')" = 1 ] || rc=1
  printf '<html></html>\n' >"$d/empty"
  extract "$d/empty" links >"$d/empty.names"
  [ ! -s "$d/empty.names" ] || rc=1
  printf '<tr id="row_0_"><td class="wrong">broken</td></tr>\n' >"$d/malformed"
  extract "$d/malformed" bare >"$d/malformed.names"
  [ ! -s "$d/malformed.names" ] || rc=1
  # Transaction/no-overwrite exercise: publish only after staged validation.
  mkdir "$d/snapshots" "$d/stage"
  printf valid >"$d/snapshots/snapshot-existing"
  if [ -s "$d/empty.names" ]; then mv "$d/stage" "$d/snapshots/snapshot-failed"; fi
  [ "$(cat "$d/snapshots/snapshot-existing")" = valid ] &&
    [ ! -e "$d/snapshots/snapshot-failed" ] || rc=1
  # Exercise the exact crawl validator: zero count, low/truncated count,
  # invalid extracted names, then confirm failed validation cannot publish.
  printf '' >"$d/zero.names"
  printf 'one\n' >"$d/truncated.names"
  printf 'bad name\n' >"$d/invalid.names"
  validate_names "$d/zero.names" 10 >/dev/null 2>&1 && rc=1
  validate_names "$d/truncated.names" 10 >/dev/null 2>&1 && rc=1
  validate_names "$d/invalid.names" 1 >/dev/null 2>&1 && rc=1
  if validate_names "$d/truncated.names" 10 >/dev/null 2>&1; then
    publish_snapshot "$d/stage" "$d/snapshots" failed || rc=1
  fi
  [ "$(cat "$d/snapshots/snapshot-existing")" = valid ] || rc=1
  # Deterministic collision: publish_snapshot must preserve both the existing
  # snapshot and the staged directory when the exact destination already exists.
  mkdir "$d/snapshots/snapshot-collision-$$" "$d/stage-collision"
  printf original >"$d/snapshots/snapshot-collision-$$/marker"
  printf staged >"$d/stage-collision/marker"
  if publish_snapshot "$d/stage-collision" "$d/snapshots" collision >/dev/null 2>&1; then
    rc=1
  fi
  [ "$(cat "$d/snapshots/snapshot-collision-$$/marker")" = original ] || rc=1
  [ "$(cat "$d/stage-collision/marker")" = staged ] || rc=1
  mkdir -p "$d/broken/repo/out"
  : >"$d/broken/repo/.git"
  created=""
  if guard_output "$d/broken/repo/out"; then rc=1; fi
  [ -e "$d/broken/repo/.git" ] || rc=1
  (
    cd "$d/broken/repo" || exit 1
    mkdir -p relative/out
    if guard_output relative/out; then exit 1; fi
    [ -e .git ] || exit 1
  ) || rc=1
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
    echo "error: cannot resolve the repository root from the script path; pass an explicit OUTPUT_DIR" >&2; exit 2; }
  OUT="$root/target/maya-catalog"
fi

created=""; d="$OUT"
while [ ! -d "$d" ]; do created="$created
$d"; d="$(dirname "$d")"; done
mkdir -p "$OUT"
if ! guard_output "$OUT"; then
  cleanup_created
  exit 1
fi

TMP="$(mktemp -d "$OUT/.staging.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT
mkdir "$TMP/pages" "$TMP/names"

fetch() { # url dest
  local attempt=1
  while ! curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' \
      --connect-timeout 20 --max-time 120 --user-agent "$UA" --output "$2" "$1"; do
    [ "$attempt" -lt 3 ] || { echo "error: giving up on $1" >&2; return 1; }
    sleep $((attempt * 5)); attempt=$((attempt + 1))
  done
}

NOW="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
MANIFEST="$TMP/manifest.tsv"
printf 'retrieved_utc\t%s\nsource_kind\tmutable year-level catalog page\ncount_references\tstructural sanity tripwire only; not completeness evidence\n' "$NOW" >"$MANIFEST"
firstfetch=1
for rel in 2027 2026; do
  suffix=""; [ "$rel" = 2026 ] && suffix="-2026"
  while IFS="|" read -r name path mode ref; do
    [ "$firstfetch" = 1 ] || sleep 2
    firstfetch=0
    url="$BASE/$rel/ENU/$path"
    fetch "$url" "$TMP/pages/$rel-$name.html"
    extract "$TMP/pages/$rel-$name.html" "$mode" >"$TMP/names/$rel-$name.names"
    validate_names "$TMP/names/$rel-$name.names" "$ref"
    count="$(wc -l <"$TMP/names/$rel-$name.names" | tr -d ' ')"
    raw_sha="$(sha256 "$TMP/pages/$rel-$name.html")"
    names_sha="$(sha256 "$TMP/names/$rel-$name.names")"
    printf 'source\t%s\t%s\t%s\t%s\t%s\t%s\tpassed\n' "$rel" "$name" "$url" "$raw_sha" "$names_sha" "$count" >>"$MANIFEST"
    echo "$rel $name: $count names"
 done <<<"$CATALOGS"
done

STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
mkdir "$TMP/publish"
cp -R "$TMP/pages" "$TMP/names" "$TMP/publish/"
cp "$MANIFEST" "$TMP/publish/manifest.tsv"
publish_snapshot "$TMP/publish" "$OUT" "$STAMP"
SNAP="$OUT/snapshot-$STAMP-$$"
echo "published snapshot $SNAP"

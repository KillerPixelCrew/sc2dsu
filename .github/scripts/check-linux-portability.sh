#!/usr/bin/env bash
# Asserts that the Linux release binary is portable across distributions.
#
# Two things break a "download and run it" binary on Linux: linking a library the user
# may not have, and requiring a glibc newer than the user's. Both regress silently the
# moment someone adds a crate with a C dependency, so they are checked here rather than
# discovered by whoever downloads the release.
#
# Usage: check-linux-portability.sh <binary> <max-glibc-version|any>
#
# Pass "any" for the glibc bound on runners whose image floats (the library allowlist,
# which is the part that catches a new C dependency, still applies).
set -euo pipefail

binary="${1:?usage: $0 <binary> <max-glibc-version>}"
max_glibc="${2:?usage: $0 <binary> <max-glibc-version>}"

# Present on every glibc system: the loader, the C runtime, libm, and libgcc.
allowed=(
  "linux-vdso.so.1"
  "ld-linux-x86-64.so.2"
  "libc.so.6"
  "libm.so.6"
  "libgcc_s.so.1"
)

status=0

echo "== dynamic dependencies of $binary =="
mapfile -t libs < <(ldd "$binary" | awk '{print $1}' | grep -v '^/' | sed 's#.*/##' | sort -u)
for lib in "${libs[@]}"; do
  [[ -z "$lib" ]] && continue
  if [[ " ${allowed[*]} " == *" $lib "* ]]; then
    echo "  ok       $lib"
  else
    echo "  NOT OK   $lib  <-- not present on every distribution"
    status=1
  fi
done

# A binary is only as portable as the newest glibc symbol it references.
echo "== glibc symbol versions =="
required="$(objdump -T "$binary" \
  | grep -oE 'GLIBC_[0-9]+\.[0-9]+' \
  | sed 's/GLIBC_//' \
  | sort -uV \
  | tail -1)"
echo "  requires glibc <= $required (limit $max_glibc)"
if [[ "$max_glibc" == "any" ]]; then
  echo "  (bound not enforced on this runner)"
elif [[ "$(printf '%s\n%s\n' "$required" "$max_glibc" | sort -V | tail -1)" != "$max_glibc" ]]; then
  echo "  NOT OK   needs glibc $required, above the $max_glibc limit"
  status=1
fi

if [[ $status -ne 0 ]]; then
  echo
  echo "FAILED: this binary is not portable across distributions."
  exit 1
fi
echo
echo "OK: links only universally present libraries (glibc <= $required)"

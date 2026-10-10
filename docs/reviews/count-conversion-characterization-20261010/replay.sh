#!/usr/bin/env bash
# Portable extracted-helper characterization; no Cargo, downloads, or crate build.
set -u
if [ "$#" -ne 1 ] || [ -z "${RUSTC:-}" ]; then
  printf '%s\n' 'Usage: RUSTC=/path/to/already-installed/rustc bash replay.sh NEW_OUTPUT_DIRECTORY' >&2
  exit 2
fi
if [ ! -x "$RUSTC" ]; then
  printf '%s\n' 'RUSTC must name an existing compiler executable; this script never installs one.' >&2
  exit 2
fi
here=$(cd -- "$(dirname -- "$0")" && pwd) || exit 2
out=$1
python3 "$here/extract-witness.py" "$out" || exit $?
"$RUSTC" -vV > "$out/compiler.txt" || exit $?
printf '%s\n' 'rustc --edition=2021 --test witness.rs -o witness-tests' 'witness-tests --nocapture --test-threads=1' > "$out/commands.txt"
(cd -- "$out" && "$RUSTC" --edition=2021 --test witness.rs -o witness-tests > compile.stdout 2> compile.stderr)
compile_status=$?
printf '%s\n' "$compile_status" > "$out/compile.exit"
if [ "$compile_status" -ne 0 ]; then
  cat "$out/compile.stderr" >&2
  exit "$compile_status"
fi
(cd -- "$out" && ./witness-tests --nocapture --test-threads=1 > test.stdout 2> test.stderr)
test_status=$?
printf '%s\n' "$test_status" > "$out/test.exit"
cat "$out/test.stdout"
cat "$out/test.stderr" >&2
# Expected current-helper outcome is exit 101, with 3 passed and 5 failed.
# Preserve that red exit instead of masking it as a successful production gate.
exit "$test_status"

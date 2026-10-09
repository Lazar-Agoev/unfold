#!/usr/bin/env bash
# Build-time gate runner for the studio redesign (deleted with docs/redesign/ at the end).
# Usage: docs/redesign/build/gates.sh [rust|js|ipc|addon|parity|oracles|all]...  (default: all)
# Prints one PASS/FAIL line per gate; full logs go to $GATE_LOGS (default: target/gate-logs).
set -uo pipefail
cd "$(dirname "$0")/../../.."
source "$HOME/.cargo/env"
logs="${GATE_LOGS:-target/gate-logs}"
mkdir -p "$logs"
status=0

run() { # name, command...
  local name=$1; shift
  if "$@" >"$logs/$name.log" 2>&1; then
    echo "PASS $name"
  else
    echo "FAIL $name  (log: $logs/$name.log)"
    grep -E 'error(\[|:)|FAILED|panicked|failed|✗|Error:' "$logs/$name.log" | head -25 | sed 's/^/    /'
    status=1
  fi
}

gates=("$@")
[ ${#gates[@]} -eq 0 ] && gates=(all)
for g in "${gates[@]}"; do
  case $g in
    rust)    run test cargo test --workspace
             run fmt cargo fmt --all -- --check
             run clippy cargo clippy --workspace --all-targets -- -D warnings ;;
    js)      run js-check just js-check ;;
    ipc)     run gen-ipc cargo xtask gen-ipc --check ;;
    addon)   run addon-test just addon-test ;;
    parity)  for c in mixed-playground playground-ts-native python-playground entrances; do
               run "parity-$c" cargo xtask parity "$c"
             done ;;
    oracles) run oracles cargo xtask oracles check ;;
    all)     "$0" rust ipc js addon parity oracles || status=1 ;;
    *) echo "unknown gate $g"; status=2 ;;
  esac
done
exit $status

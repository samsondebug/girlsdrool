#!/usr/bin/env bash
# Greppable quality gates from the spec. Each failing gate prints the offending lines; the script
# exits non-zero if any gate failed. Kept deliberately simple so it runs identically on Windows
# (Git Bash) and Linux.
set -uo pipefail

fail=0
gate() {
  # gate <name> <command...>: the command's output lists offenders; any output is a failure.
  local name="$1"; shift
  local out
  out="$("$@" 2>/dev/null || true)"
  if [[ -n "$out" ]]; then
    echo "GATE FAIL — $name"
    echo "$out"
    echo
    fail=1
  fi
}

core_src="src-tauri/src"
web_src="src"

gate "TODO / FIXME / XXX markers" \
  grep -rnE '\b(TODO|FIXME|XXX)\b' "$core_src" "$web_src" src-tauri/migrations e2e \
    --include='*.rs' --include='*.ts' --include='*.tsx' --include='*.css' --include='*.sql'

gate "\`any\` in TypeScript" \
  grep -rnE '(:\s*any\b|<any>|\bas any\b|\bany\[\])' "$web_src" e2e --include='*.ts' --include='*.tsx'

# unwrap()/expect( are allowed only in #[cfg(test)] modules and in tests/.
gate "unwrap()/expect( outside tests in the core" \
  awk '
    FNR == 1 { in_tests = 0 }
    /#\[cfg\(test\)\]/ { in_tests = 1 }
    !in_tests && /\.(unwrap\(\)|expect\()/ { printf "%s:%d: %s\n", FILENAME, FNR, $0 }
  ' $(find "$core_src" -name '*.rs')

gate "println! in the core (use tracing)" \
  grep -rnE '(^|[^e])println!' "$core_src"

# Float types in code (comment lines may mention them when stating the rule).
floats_in_code() {
  grep -rnE '\b(f32|f64)\b' "$core_src" | grep -vE '^[^:]+:[0-9]+:\s*//'
}
gate "float types in the core (money is i64 cents)" floats_in_code

gate "network APIs in the webview" \
  grep -rnE '\bfetch\(|XMLHttpRequest|WebSocket|sendBeacon|navigator\.onLine|EventSource\(' "$web_src"

# Network and shell crates must be absent from the resolved dependency graph of the shipping
# target (Windows) and of the host. Cargo.lock is not inspected: it also lists mobile-only
# dependencies of Tauri that never link into a desktop build.
network_crates_in_graph() {
  local crates=(reqwest ureq isahc curl tauri-plugin-http tauri-plugin-updater tauri-plugin-shell tauri-plugin-websocket)
  local targets=("" "--target x86_64-pc-windows-msvc")
  for crate in "${crates[@]}"; do
    for target in "${targets[@]}"; do
      # shellcheck disable=SC2086
      if cargo tree --manifest-path src-tauri/Cargo.toml --all-features $target -i "$crate" --depth 1 2>/dev/null | grep -qE "^$crate "; then
        echo "$crate is in the dependency graph (${target:-host})"
      fi
    done
  done
}
gate "network or shell crates in the dependency graph" network_crates_in_graph

gate "network plugins or updater in tauri.conf.json" \
  grep -nE '"(updater|http|shell)"' src-tauri/tauri.conf.json

# Commented-out code: a line comment that looks like a statement.
gate "commented-out code" \
  grep -rnE '^\s*//\s*(let|const|fn|pub|use|import|export|return|if|for|while|match|await|console)\b.*[;{}]\s*$' \
    "$core_src" "$web_src" e2e --include='*.rs' --include='*.ts' --include='*.tsx'

if [[ $fail -ne 0 ]]; then
  echo "gates: FAILED"
  exit 1
fi
echo "gates: clean"

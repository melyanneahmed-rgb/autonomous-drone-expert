#!/usr/bin/env bash
# Pre-packaging guard for the m1b runner: refuse to package if the source contains an
# ACTUAL payload-write or control-line CALL.
#
# Matching rule: method-call syntax only -- a dot, one of the named functions, optional
# whitespace, then an opening parenthesis -- evaluated after stripping `//` comments.
# Plain words such as "write/write_all/flush" inside prose or string literals (the
# safety banner) do NOT trip it. The previous revision matched bare words and rejected
# its own safety text; that failure is selftest case 3 below.
#
# Honest limitation: this is still a regex tripwire, not a parser. A string literal
# containing exact call syntax would false-positive, and a call split across lines
# would evade it. It is a packaging backstop; the audited source and human review
# remain the authoritative control. Spike tooling only -- never promoted to main.
set -euo pipefail

PATTERN='\.(write|write_all|write_some|write_vectored|write_all_with_deadline|flush|discard_[a-z_]*|purge|set_dtr|set_rts|write_data_terminal_ready|write_request_to_send)[[:space:]]*\('

check_file() {
  # Returns 0 when clean; prints offending lines and returns 1 when a call is found.
  sed 's|//.*||' "$1" | grep -nE "$PATTERN" && return 1
  return 0
}

cmd="${1:-}"
file="${2:-}"
[ -n "$cmd" ] && [ -n "$file" ] || { echo "usage: $0 {check|selftest} FILE"; exit 2; }

case "$cmd" in
  check)
    if check_file "$file"; then
      echo "guard: clean -- no payload-write or control-line calls in $file"
    else
      echo "guard: payload-write or control-line CALL found in $file -- refusing to package"
      exit 1
    fi
    ;;
  selftest)
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT

    # 1. the real runner source must pass
    if check_file "$file" >/dev/null; then
      echo "selftest 1: real $file passes .............. OK"
    else
      echo "selftest 1 FAILED: real $file was rejected"; exit 1
    fi

    # 2. an actual write call must fail
    printf 'fn f(mut port: P, data: &[u8]) { port.write(data); }\n' > "$tmp/call.rs"
    if check_file "$tmp/call.rs" >/dev/null; then
      echo "selftest 2 FAILED: port.write(data) was accepted"; exit 1
    else
      echo "selftest 2: port.write(data) fails ......... OK"
    fi

    # 3. the words inside a string literal must pass
    printf 'const S: &str = "No write/write_all/flush";\n' > "$tmp/text.rs"
    if check_file "$tmp/text.rs" >/dev/null; then
      echo "selftest 3: banner-style string passes ..... OK"
    else
      echo "selftest 3 FAILED: string literal was rejected"; exit 1
    fi

    # 4. an actual flush call must fail
    printf 'fn f(mut port: P) { port.flush(); }\n' > "$tmp/flush.rs"
    if check_file "$tmp/flush.rs" >/dev/null; then
      echo "selftest 4 FAILED: port.flush() was accepted"; exit 1
    else
      echo "selftest 4: port.flush() fails ............. OK"
    fi

    echo "guard selftest: 4/4 OK"
    ;;
  *)
    echo "usage: $0 {check|selftest} FILE"; exit 2 ;;
esac

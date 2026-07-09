#!/usr/bin/env bash
# Approximate cold-start timing for DopePad.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${DOPEPAD_BIN:-}"
APP_ID="io.github.phpont.DopePad"

if [[ -z "$BIN" ]]; then
  if [[ -x "${ROOT}/target/release/dopepad" ]]; then
    BIN="${ROOT}/target/release/dopepad"
  elif command -v dopepad >/dev/null 2>&1; then
    BIN="$(command -v dopepad)"
  else
    echo "No dopepad binary found. Build with: cargo build --release" >&2
    exit 1
  fi
fi

MODE="${1:---daily}"
RUNS="${BENCH_RUNS:-3}"

echo "Binary: ${BIN}"
echo "Mode:   ${MODE}"
echo "Runs:   ${RUNS}"
echo ""

have_niri_msg=0
if command -v niri >/dev/null 2>&1 && niri msg --json windows >/dev/null 2>&1; then
  have_niri_msg=1
fi

window_visible() {
  if [[ "$have_niri_msg" -eq 1 ]]; then
    niri msg --json windows 2>/dev/null | grep -Fq "\"app_id\":\"${APP_ID}\""
  else
    return 1
  fi
}

times=()

for ((i = 1; i <= RUNS; i++)); do
  pkill -x dopepad >/dev/null 2>&1 || true
  # Wait until previous window is gone
  for _ in $(seq 1 50); do
    window_visible || break
    sleep 0.02
  done
  sleep 0.1

  start_ns="$(date +%s%N)"

  if [[ "$have_niri_msg" -eq 1 ]]; then
    "$BIN" "$MODE" >/dev/null 2>&1 &
    pid=$!
    deadline_ns=$((start_ns + 10000000000)) # 10s
    appeared=0
    while true; do
      now_ns="$(date +%s%N)"
      if window_visible; then
        appeared=1
        break
      fi
      if ! kill -0 "$pid" 2>/dev/null; then
        break
      fi
      if (( now_ns > deadline_ns )); then
        break
      fi
      sleep 0.005
    done
    end_ns="$(date +%s%N)"
    pkill -x dopepad >/dev/null 2>&1 || true
    if [[ "$appeared" -ne 1 ]]; then
      echo "run ${i}: window not detected via niri app_id=${APP_ID}"
    fi
  else
    if [[ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]]; then
      "$BIN" "$MODE" >/dev/null 2>&1 &
      pid=$!
      # Fallback: process stays up and has had time to present
      for _ in $(seq 1 400); do
        if ! kill -0 "$pid" 2>/dev/null; then
          break
        fi
        now_ns="$(date +%s%N)"
        if (( now_ns - start_ns > 80000000 )); then
          sleep 0.05
          break
        fi
        sleep 0.005
      done
      end_ns="$(date +%s%N)"
      pkill -x dopepad >/dev/null 2>&1 || true
    else
      echo "No display / niri available; measuring binary --help only."
      start_ns="$(date +%s%N)"
      "$BIN" --help >/dev/null
      end_ns="$(date +%s%N)"
    fi
  fi

  elapsed_ms=$(( (end_ns - start_ns) / 1000000 ))
  times+=("$elapsed_ms")
  echo "run ${i}: ${elapsed_ms} ms"
  sleep 0.25
done

sum=0
for t in "${times[@]}"; do
  sum=$((sum + t))
done
avg=$((sum / RUNS))

echo ""
echo "average: ${avg} ms"
if (( avg < 700 )); then
  echo "status:  cold-start target (<700 ms) OK"
else
  echo "status:  cold-start above 700 ms target — investigate"
fi

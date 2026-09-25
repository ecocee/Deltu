#!/usr/bin/env bash
# End-to-end demo (spec 14): simulated sensor → HTTP → pipeline → state →
# rule → action. Reproducible proof of the central principle: process data
# first, AI only when necessary (no AI in this path at all).
#
# Usage: examples/demo.sh [port]     (default 8211)
set -euo pipefail

PORT="${1:-8211}"
BASE="http://127.0.0.1:${PORT}"
CONFIG="$(mktemp /tmp/deltu-demo-XXXXXX.yaml)"
TS=$(python3 -c "import time; print(int(time.time()*1000))")

cat > "$CONFIG" <<EOF
http:
  bind: 127.0.0.1:${PORT}
rules:
  - id: door-open-log
    on: !Event
      kind: door
    condition: !Comparison
      field: !EventValue
      op: Eq
      value: !Text
        open
    action: demo-log
actions:
  - id: demo-log
    kind:
      type: log
      level: info
EOF

cleanup() { kill "${ENGINE_PID:-0}" 2>/dev/null || true; rm -f "$CONFIG"; }
trap cleanup EXIT

echo "== starting engine on :${PORT} =="
target/debug/deltu run --config "$CONFIG" &
ENGINE_PID=$!
sleep 1.5

echo "== health =="
curl -fsS "$BASE/health"; echo

echo "== sending simulated sensor events =="
curl -fsS -X POST "$BASE/v1/events" -H 'content-type: application/json' -d "{
  \"events\": [
    {\"id\":\"d-1\",\"source\":\"sim-sensor\",\"kind\":\"door.state\",\"timestamp\":${TS},\"payload\":{\"type\":\"text\",\"value\":\"open\"}},
    {\"id\":\"d-2\",\"source\":\"sim-sensor\",\"kind\":\"door.state\",\"timestamp\":${TS},\"payload\":{\"type\":\"text\",\"value\":\"closed\"}},
    {\"id\":\"d-3\",\"source\":\"sim-sensor\",\"kind\":\"temperature.reading\",\"timestamp\":${TS},\"payload\":{\"type\":\"numeric\",\"value\":21.5}}
  ]
}"; echo

echo "== status =="
curl -fsS "$BASE/v1/status" | python3 -m json.tool

echo "== expected: accepted [true,true,true]; actions_fired 1 (rule 'door-open-log' on d-1);"
echo "   state.entries 1 (door.state open→closed share one key; numeric accumulates, no entry yet) =="

#!/usr/bin/env python3
import json
import sys

try:
    from CoolProp.CoolProp import PropsSI
except Exception as exc:  # noqa: BLE001
    STARTUP_ERROR = str(exc)
    PropsSI = None
else:
    STARTUP_ERROR = None


def emit(payload: dict) -> None:
    sys.stdout.write(json.dumps(payload) + "\n")
    sys.stdout.flush()


def classify_error(message: str) -> str:
    lower = message.lower()
    if "unable to match the key" in lower or "not found" in lower or "not a valid fluid" in lower:
        return "unknown_fluid"
    if "input pair" in lower or "pair" in lower:
        return "invalid_pair"
    if "out of range" in lower or "unable to solve" in lower:
        return "out_of_range"
    return "backend"


for raw_line in sys.stdin:
    line = raw_line.strip()
    if not line:
        continue

    if STARTUP_ERROR is not None:
        emit({"ok": False, "kind": "backend", "message": f"CoolProp import failed: {STARTUP_ERROR}"})
        continue

    try:
        req = json.loads(line)
        fluid = req["fluid"]
        out = req["out"]
        in1 = req["in1"]
        in2 = req["in2"]
        value = float(PropsSI(out, in1["var"], in1["value"], in2["var"], in2["value"], fluid))
        emit({"ok": True, "value": value})
    except Exception as exc:  # noqa: BLE001
        msg = str(exc)
        emit({"ok": False, "kind": classify_error(msg), "message": msg})

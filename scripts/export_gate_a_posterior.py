#!/usr/bin/env python3
"""One-time Gate-A fixture export for the SpecERE Rust parity test.

Run against the ReSearch prototype (must be importable) to produce a
committed TOML fixture at
`crates/specere-filter/tests/fixtures/gate_a/posterior.toml`.

The fixture contains:

  - seed + steps + filter name metadata.
  - Observable trace: every write's `files_touched` and every test's
    (spec_id, outcome). Reads are excluded — the Rust port has no
    ReadSensor (Phase 4 scope), and the prototype's read path contributes
    only weak evidence that doesn't meaningfully change tail-MAP.
  - Final per-spec marginals (length-3 vectors) from the prototype's
    `PerSpecHMM` after the trace.
  - Ground-truth snapshot at the final step (for the Rust test's
    MAP-accuracy assertion).

Regenerate only when algorithmic priors change. Each regeneration invalidates
the Rust parity assertions until `gate_a_parity.rs` is re-pinned.

Usage:
    python3 scripts/export_gate_a_posterior.py

Requires `ReSearch/prototype/mini_specs/` to be importable. By default we
expect ReSearch to live at `../ReSearch` next to this repo; override with
`SPECERE_RESEARCH_PATH=/path/to/ReSearch python3 scripts/...`.
"""
from __future__ import annotations

import os
import sys
from pathlib import Path

import numpy as np

# Add ReSearch to path so `from prototype.mini_specs import ...` works.
HERE = Path(__file__).resolve().parent
SPECERE_ROOT = HERE.parent
DEFAULT_RESEARCH = SPECERE_ROOT.parent / "ReSearch"
RESEARCH_PATH = Path(os.environ.get("SPECERE_RESEARCH_PATH", str(DEFAULT_RESEARCH)))
if not RESEARCH_PATH.exists():
    sys.exit(
        f"cannot find ReSearch repo at {RESEARCH_PATH}; set SPECERE_RESEARCH_PATH to override"
    )
sys.path.insert(0, str(RESEARCH_PATH))

from prototype.mini_specs.world import build_demo_world, ScriptedAgent  # noqa: E402
from prototype.mini_specs.sensors import TestSensor  # noqa: E402
from prototype.mini_specs.filter import PerSpecHMM  # noqa: E402


SEED = 0
STEPS = 120
OUT = SPECERE_ROOT / "crates" / "specere-filter" / "tests" / "fixtures" / "gate_a" / "posterior.toml"


def escape_toml_string(s: str) -> str:
    return s.replace("\\", "\\\\").replace('"', '\\"')


def emit_toml(
    specs: list[str],
    supports: dict[str, list[str]],
    trace: list[dict],
    final_beliefs: np.ndarray,
    final_gt: list[int],
    writer,
) -> None:
    writer.write("# Gate-A fixture — exported from ReSearch/prototype/mini_specs/ via\n")
    writer.write("# scripts/export_gate_a_posterior.py. Do not hand-edit.\n\n")
    writer.write(f"seed = {SEED}\n")
    writer.write(f"steps = {STEPS}\n")
    writer.write('filter = "PerSpecHMM"\n')
    quoted = ", ".join('"' + s + '"' for s in specs)
    writer.write(f"specs = [{quoted}]\n\n")

    writer.write("[supports]\n")
    for sid in specs:
        paths = ", ".join(f'"{escape_toml_string(p)}"' for p in supports[sid])
        writer.write(f'"{sid}" = [{paths}]\n')
    writer.write("\n")

    writer.write("[[trace]]\n")
    for i, e in enumerate(trace):
        if i > 0:
            writer.write("[[trace]]\n")
        if e["kind"] == "write":
            paths = ", ".join(f'"{escape_toml_string(p)}"' for p in e["files"])
            writer.write('kind = "write"\n')
            writer.write(f"files = [{paths}]\n")
        elif e["kind"] == "test":
            writer.write('kind = "test"\n')
            writer.write(f'spec_id = "{e["spec_id"]}"\n')
            writer.write(f'outcome = "{e["outcome"]}"\n')
        else:
            raise ValueError(f"unknown kind: {e['kind']}")

    writer.write("\n[expected]\n")
    for i, sid in enumerate(specs):
        p_unk, p_sat, p_vio = final_beliefs[i]
        writer.write(f'"{sid}" = {{ p_unk = {p_unk:.12f}, p_sat = {p_sat:.12f}, p_vio = {p_vio:.12f}, gt = {final_gt[i]} }}\n')


def main() -> None:
    world = build_demo_world(seed=SEED)
    agent = ScriptedAgent(world, rng=np.random.default_rng(SEED + 10))
    test_sensor = TestSensor(world, rng=np.random.default_rng(SEED + 20))
    filt = PerSpecHMM(world)

    specs = [s.id for s in world.specs]
    supports = {s.id: list(s.support) for s in world.specs}

    trace: list[dict] = []

    for _ in range(STEPS):
        tool_calls = agent.emit_step()
        # The agent's emit_step already mutates the world via apply_write
        # during the write step — we record + replay post-facto.
        for tc in tool_calls:
            if tc.kind == "write":
                files_touched = list(tc.args["files"])
                trace.append({"kind": "write", "files": files_touched})
                filt.predict(set(files_touched))
            elif tc.kind == "test":
                spec_id = tc.args["spec_id"]
                outcome = test_sensor.observe(spec_id)
                trace.append({"kind": "test", "spec_id": spec_id, "outcome": outcome})
                filt.update_test(spec_id, outcome, test_sensor)
            # reads intentionally dropped

    final_beliefs = filt.all_marginals()
    final_gt = [int(s.status) for s in world.specs]

    OUT.parent.mkdir(parents=True, exist_ok=True)
    with OUT.open("w") as fh:
        emit_toml(specs, supports, trace, final_beliefs, final_gt, fh)

    print(f"wrote {OUT}")
    print(f"  trace events: {len(trace)}")
    tail_map = np.argmax(final_beliefs, axis=1)
    n_correct = sum(1 for i, g in enumerate(final_gt) if tail_map[i] == g)
    print(f"  tail-MAP accuracy: {n_correct}/{len(specs)}")


if __name__ == "__main__":
    main()

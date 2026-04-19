#!/usr/bin/env python3
"""One-time Gate-A fixture export for the SpecERE Rust parity test.

Runs the three prototype filter variants — `PerSpecHMM`, `FactorGraphBP`,
`RBPF` — on a shared Gate-A trace and writes their final posteriors to
`crates/specere-filter/tests/fixtures/gate_a/posterior.toml`. The Rust
parity tests (`crates/specere-filter/tests/gate_a_parity.rs`) replay
the trace and assert each Rust filter matches its prototype counterpart
within FR-P4-002's 2-percentage-point tolerance.

Fixture sections:

    seed, steps, specs, supports, coupling, cluster
    [[trace]]          observable events (write + test) used by all three filters
    [expected_per_spec_hmm.<id>] = { p_unk, p_sat, p_vio, gt }
    [expected_factor_graph_bp.<id>] = { p_unk, p_sat, p_vio }
    [expected_rbpf.<id>]       = { p_unk, p_sat, p_vio }

Reads are excluded — the Rust port has no ReadSensor in v1.0, and the
prototype's `update_read` contributes only weak evidence.

Regenerate only when algorithmic priors change. Each regeneration
invalidates the Rust parity assertions until `gate_a_parity.rs` re-pins.

Usage:
    python3 scripts/export_gate_a_posterior.py

Requires `ReSearch/prototype/mini_specs/` importable. By default we
expect `ReSearch` at `../ReSearch` next to this repo; override with
`SPECERE_RESEARCH_PATH=/path/to/ReSearch python3 scripts/...`.
"""
from __future__ import annotations

import os
import sys
from pathlib import Path

import numpy as np

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
from prototype.mini_specs.filter import PerSpecHMM, FactorGraphBP, RBPF  # noqa: E402


SEED = 0
STEPS = 120
RBPF_SEED = 11
RBPF_CLUSTER = [
    "auth_session",
    "billing_charge",
    "billing_refund",
    "user_create",
    "user_update",
]
OUT = SPECERE_ROOT / "crates" / "specere-filter" / "tests" / "fixtures" / "gate_a" / "posterior.toml"


def escape_toml_string(s: str) -> str:
    return s.replace("\\", "\\\\").replace('"', '\\"')


def write_expected(writer, section: str, specs, beliefs, include_gt=False, gt=None):
    writer.write(f"\n[{section}]\n")
    for i, sid in enumerate(specs):
        p_unk, p_sat, p_vio = beliefs[i]
        if include_gt:
            writer.write(
                f'"{sid}" = {{ p_unk = {p_unk:.12f}, p_sat = {p_sat:.12f}, '
                f'p_vio = {p_vio:.12f}, gt = {gt[i]} }}\n'
            )
        else:
            writer.write(
                f'"{sid}" = {{ p_unk = {p_unk:.12f}, p_sat = {p_sat:.12f}, '
                f'p_vio = {p_vio:.12f} }}\n'
            )


def emit_toml(
    specs, supports, coupling, cluster, trace,
    beliefs_hmm, beliefs_bp, beliefs_rbpf, final_gt, writer,
):
    writer.write("# Gate-A fixture — exported from ReSearch/prototype/mini_specs/ via\n")
    writer.write("# scripts/export_gate_a_posterior.py. Do not hand-edit.\n\n")
    writer.write(f"seed = {SEED}\n")
    writer.write(f"steps = {STEPS}\n")
    writer.write(f"rbpf_seed = {RBPF_SEED}\n")
    writer.write(f"rbpf_n_particles = 256\n")
    quoted = ", ".join('"' + s + '"' for s in specs)
    writer.write(f"specs = [{quoted}]\n")
    quoted_cluster = ", ".join('"' + s + '"' for s in cluster)
    writer.write(f"cluster = [{quoted_cluster}]\n\n")

    writer.write("[supports]\n")
    for sid in specs:
        paths = ", ".join(f'"{escape_toml_string(p)}"' for p in supports[sid])
        writer.write(f'"{sid}" = [{paths}]\n')
    writer.write("\n")

    writer.write("[coupling]\n")
    edges = []
    for src, dsts in coupling.items():
        for dst in dsts:
            edges.append((src, dst))
    edge_strs = ", ".join(f'["{s}", "{d}"]' for s, d in edges)
    writer.write(f"edges = [{edge_strs}]\n")

    writer.write("\n[[trace]]\n")
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

    write_expected(writer, "expected_per_spec_hmm", specs, beliefs_hmm,
                   include_gt=True, gt=final_gt)
    write_expected(writer, "expected_factor_graph_bp", specs, beliefs_bp)
    write_expected(writer, "expected_rbpf", specs, beliefs_rbpf)


def collect_trace_and_gt(world, agent, test_sensor, steps: int):
    """Single pass over the scripted agent — records observable events
    (write + test) and mutates the world. The resulting trace is
    filter-agnostic: we replay it through each filter separately below.
    GT snapshots are taken from the world-mutation sequence for the
    PerSpecHMM output only (MAP-accuracy metadata)."""
    trace = []
    for _ in range(steps):
        tool_calls = agent.emit_step()
        for tc in tool_calls:
            if tc.kind == "write":
                trace.append({"kind": "write", "files": list(tc.args["files"])})
            elif tc.kind == "test":
                spec_id = tc.args["spec_id"]
                outcome = test_sensor.observe(spec_id)
                trace.append({"kind": "test", "spec_id": spec_id, "outcome": outcome})
    final_gt = [int(s.status) for s in world.specs]
    return trace, final_gt


def run_filter_on_trace(filt, trace):
    """Drive a prototype filter through a pre-recorded trace. `filt` must
    already be constructed with the same world/coupling that generated the
    trace."""
    for e in trace:
        if e["kind"] == "write":
            filt.predict(set(e["files"]))
        elif e["kind"] == "test":
            # Re-use the same test_sensor-less call path: the prototype's
            # `update_test` only needs the sensor's `log_likelihood` table,
            # which is deterministic given alpha constants. We pass a
            # freshly-built TestSensor per filter so the flake RNG state
            # doesn't leak across filters. For FR-P4-002 parity we bypass
            # the sensor's stochastic `observe()` — the outcome was already
            # recorded in the trace.
            filt.update_test(e["spec_id"], e["outcome"], filt.world_sensor)


def build_world_with_sensor(seed):
    world = build_demo_world(seed=seed)
    # Attach a TestSensor the same way PerSpecHMM/BP use in demo.py.
    test_sensor = TestSensor(world, rng=np.random.default_rng(seed + 20))
    return world, test_sensor


def main():
    # Phase 1: collect the shared trace using a vanilla world + agent.
    world_trace, ts_trace = build_world_with_sensor(SEED)
    agent = ScriptedAgent(world_trace, rng=np.random.default_rng(SEED + 10))
    trace, final_gt = collect_trace_and_gt(world_trace, agent, ts_trace, STEPS)
    specs = [s.id for s in world_trace.specs]
    supports = {s.id: list(s.support) for s in world_trace.specs}
    coupling = dict(world_trace.coupling)

    # Phase 2: replay through each filter on a FRESH world (world state
    # doesn't matter to the filter — filters only see observations — but a
    # fresh build avoids any mutation side-effects influencing the filter's
    # internal sensor-table construction).

    # PerSpecHMM — deterministic
    w1, s1 = build_world_with_sensor(SEED)
    hmm = PerSpecHMM(w1)
    hmm.world_sensor = s1  # stash for run_filter_on_trace
    run_filter_on_trace(hmm, trace)
    beliefs_hmm = hmm.all_marginals()

    # FactorGraphBP — deterministic
    w2, s2 = build_world_with_sensor(SEED)
    bp = FactorGraphBP(w2)
    bp.world_sensor = s2
    run_filter_on_trace(bp, trace)
    beliefs_bp = bp.all_marginals()

    # RBPF — stochastic; uses its own seeded rng
    w3, s3 = build_world_with_sensor(SEED)
    rbpf = RBPF(w3, cluster_spec_ids=RBPF_CLUSTER,
                n_particles=256, rng=np.random.default_rng(RBPF_SEED))
    rbpf.world_sensor = s3
    run_filter_on_trace(rbpf, trace)
    beliefs_rbpf = rbpf.all_marginals()

    OUT.parent.mkdir(parents=True, exist_ok=True)
    with OUT.open("w") as fh:
        emit_toml(specs, supports, coupling, RBPF_CLUSTER, trace,
                  beliefs_hmm, beliefs_bp, beliefs_rbpf, final_gt, fh)

    print(f"wrote {OUT}")
    print(f"  trace events: {len(trace)}")
    for name, b in [("HMM", beliefs_hmm), ("BP", beliefs_bp), ("RBPF", beliefs_rbpf)]:
        tail_map = np.argmax(b, axis=1)
        n_correct = sum(1 for i, g in enumerate(final_gt) if tail_map[i] == g)
        print(f"  {name}: tail-MAP accuracy {n_correct}/{len(specs)}")


if __name__ == "__main__":
    main()

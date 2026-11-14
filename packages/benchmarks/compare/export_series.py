"""Write the per-second series the /benchmarks page draws.

    python3 compare/export_series.py results/compare/round1 ../../apps/docs/src/lib/compare-series.json

For every <product>.json in the run: stored events and stack memory, with time
in seconds from the first request of the burst.
"""

import csv
import glob
import json
import os
import sys

run_dir, out = sys.argv[1], sys.argv[2]
series = {}
for summary in sorted(glob.glob(f"{run_dir}/*-phases.csv")):
    product = os.path.basename(summary)[: -len("-phases.csv")]
    if not os.path.exists(f"{run_dir}/{product}.json"):
        continue
    p = f"{run_dir}/{product}"
    phases = {name: int(t) for t, name in csv.reader(open(f"{p}-phases.csv"))}
    t0 = phases["load"]
    stored = [(int(t) - t0, int(n)) for t, n in csv.reader(open(f"{p}-stored.csv"))]
    base = stored[0][1] if stored and stored[0][0] <= 0 else 0
    final = max(n for _, n in stored)
    done = next(t for t, n in stored if n == final)
    end = done + 20
    mem = [
        (int(t) - t0, round(int(a) / 2**20, 1))
        for t, a, _, _ in csv.reader(open(f"{p}-mem.csv"))
        if -20 <= int(t) - t0 <= end
    ]
    series[product] = {
        "stored": [[0, 0]] + [[t, n - base] for t, n in stored if 0 < t <= end],
        "memory": mem,
    }
json.dump(series, open(out, "w"), separators=(",", ":"))
print({k: (len(v["stored"]), len(v["memory"])) for k, v in series.items()})

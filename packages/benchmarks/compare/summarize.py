"""Turn one product's raw files from run.sh into <product>.json and a one-line print.

Reads <out>/<product>-{mem,phases,stored}.csv, the load generator's JSON,
docker's df output and the image list. Memory is cgroup `anon` summed over the
stack's containers; CPU is cgroup `usage_usec` summed the same way.
"""

import csv
import glob
import json
import re
import statistics
import sys

out, product = sys.argv[1], sys.argv[2]
p = f"{out}/{product}"

mem = [(int(t), int(a), int(c)) for t, a, c, _ in csv.reader(open(f"{p}-mem.csv"))]
phases = {name: int(t) for t, name in csv.reader(open(f"{p}-phases.csv"))}
stored = [(int(t), int(n)) for t, n in csv.reader(open(f"{p}-stored.csv"))]
load = json.load(open(sorted(glob.glob(f"{p}-load/*.json"))[-1]))["results"]


def window(start, end):
    return [s for s in mem if start <= s[0] < end]


def mb(values):
    return round(values / 2**20, 1)


idle = window(phases["load"] - 15, phases["load"])
busy = window(phases["load"], phases["floor"])
floor = window(phases["end"] - 15, phases["end"] + 1)

final = stored[-1][1]
# The first poll that reached the final count: when the last event was stored.
done_at = next(t for t, n in stored if n == final)
processing_secs = max(done_at - phases["load"], 1)

cpu_start = window(phases["load"] - 2, phases["load"] + 1)[0][2]
cpu_end = max(s[2] for s in window(done_at - 1, done_at + 3) or busy)

UNITS = {"B": 1, "kB": 1e3, "KB": 1e3, "MB": 1e6, "GB": 1e9, "TB": 1e12}


def size(text):
    number, unit = re.fullmatch(r"([\d.]+)\s*([kKMGT]?B)", text).groups()
    return float(number) * UNITS[unit]


df = json.load(open(f"{p}-df.json"))
project = "sentry-self-hosted" if product.startswith("sentry") else f"cmp-{product}"
# Sentry's data volumes are external and named sentry-*, outside its project.
volumes = [
    v
    for v in df.get("Volumes", [])
    if v["Name"].startswith(project + "_")
    or (product.startswith("sentry") and v["Name"].startswith("sentry-"))
]
images = [line.strip() for line in open(f"{p}-images.txt") if line.strip()]
image_bytes = 0
for image in df.get("Images", []):
    tag = f'{image["Repository"]}:{image["Tag"]}'
    if tag in images or (image["Tag"] == "latest" and image["Repository"] in images):
        image_bytes += size(image["Size"])

result = {
    "product": product,
    "containers": int(open(f"{p}-containers").read()),
    "images": images,
    "image_mb": round(image_bytes / 1e6),
    "ready_secs": int(open(f"{p}-ready-secs").read()),
    "anon_mb": {
        "idle": mb(statistics.median(s[1] for s in idle)),
        "peak": mb(max(s[1] for s in busy)),
        "floor": mb(statistics.median(s[1] for s in floor)),
    },
    "load": {
        "sent": load["throughput"]["total_requests"],
        "accepted": load["throughput"]["successful"],
        "failed": load["throughput"]["failed"],
        "accept_per_s": round(load["throughput"]["events_per_second"]),
        "latency_ms": {k: load["latency_ms"][k] for k in ("p50", "p99", "max")},
        "errors": load.get("errors"),
    },
    "stored": final,
    "processing_secs": processing_secs,
    "stored_per_s": round(final / processing_secs, 1),
    "cpu_ms_per_event": round((cpu_end - cpu_start) / 1000 / max(final, 1), 2),
    "disk_mb": round(sum(size(v["Size"]) for v in volumes) / 1e6, 1),
}
json.dump(result, open(f"{p}.json", "w"), indent=2)
print(
    f'{product}: {result["containers"]} containers, anon idle/peak/floor '
    f'{result["anon_mb"]["idle"]}/{result["anon_mb"]["peak"]}/{result["anon_mb"]["floor"]} MB, '
    f'accepted {result["load"]["accepted"]}/{result["load"]["sent"]} at {result["load"]["accept_per_s"]}/s, '
    f'stored {final} in {processing_secs} s ({result["stored_per_s"]}/s), '
    f'{result["cpu_ms_per_event"]} CPU ms/event, disk {result["disk_mb"]} MB'
)

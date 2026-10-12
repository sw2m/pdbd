#!/usr/bin/env python3
# The GHA check matrix is derived from lefthook (the ci hook's fan-out leaves), never hardcoded.
import glob, json, os, subprocess


def dump(cfg=None):
    env = dict(os.environ)
    if cfg:
        env["LEFTHOOK_CONFIG"] = cfg
    out = subprocess.run(
        ["lefthook", "dump", "--format", "json"],
        capture_output=True, text=True, env=env, check=True,
    ).stdout
    return json.loads(out)


root = dump()
ci = [j["name"] for j in root["ci"]["jobs"]]
matrix = []

for x in ci:
    h = f"{x}:root"
    jobs = root.get(h, {}).get("jobs", [])
    if jobs and not all(j.get("run") == "true" for j in jobs):
        matrix.append({"name": h, "config": "lefthook.yml", "hook": h})

for cfg in sorted(glob.glob("crates/*/lefthook.yml")):
    crate = cfg.split("/")[1]
    hooks = dump(cfg)
    for x in ci:
        if x in hooks:
            matrix.append({"name": f"{x} ({crate})", "config": cfg, "hook": x})

print(json.dumps(matrix))

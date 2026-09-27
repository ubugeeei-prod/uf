#!/usr/bin/env python3
"""Controlled native PM smoke checks and install medians, recorded as CI artifacts."""
import argparse
import json
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import tempfile
import time

FIXTURES = {
    "small": {"react": "19.3.0", "react-dom": "19.3.0", "chalk": "6.0.1"},
    "application": {"react": "19.3.0", "react-dom": "19.3.0", "vite": "8.3.1", "lucide-react": "1.48.0"},
}


def invoke(argv, directory, environment, expected=0):
    begin = time.perf_counter_ns()
    result = subprocess.run(argv, cwd=directory, env=environment, capture_output=True, text=True, timeout=180)
    elapsed = (time.perf_counter_ns() - begin) / 1_000_000
    if result.returncode != expected:
        raise RuntimeError(f"{argv} exited {result.returncode}\n{result.stdout[-6000:]}\n{result.stderr[-6000:]}")
    return elapsed, result


def project(directory, deps, manager):
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "package.json").write_text(json.dumps({"name": "pm-fixture", "private": True, "dependencies": deps}))
    (directory / "uf.config.js").write_text(f"export default {{ packageManager: '{manager}' }};\n")


def disk_usage(directories):
    seen = set()
    allocated = logical = files = 0
    for directory in directories:
        if not directory.exists():
            continue
        for root, _, names in os.walk(directory, followlinks=False):
            for name in names:
                path = Path(root) / name
                stat = path.lstat()
                if path.is_symlink() or not path.is_file() or (stat.st_dev, stat.st_ino) in seen:
                    continue
                seen.add((stat.st_dev, stat.st_ino))
                files += 1
                logical += stat.st_size
                allocated += stat.st_blocks * 512
    return {"allocatedBytes": allocated, "logicalBytes": logical, "uniqueFiles": files}


def smoke(uf, root, environment):
    directory = root / "security"
    project(directory, {"lodash": "4.17.20"}, "uf")
    invoke([uf, "install"], directory, environment)
    before = (directory / "uf.lock").read_bytes()
    invoke([uf, "install", "--frozen-lockfile"], directory, environment)
    assert (directory / "uf.lock").read_bytes() == before
    _, audited = invoke([uf, "audit", "--json"], directory, environment, expected=1)
    findings = json.loads(audited.stdout)
    assert findings["checked"] == 1
    assert any(row["package"] == "lodash" and "4.17.20" in row["versions"] for row in findings["findings"])
    assert any(row["fixVersion"] for row in findings["findings"])
    invoke(["node", "-e", "const l=require('lodash'); if(l.chunk([1,2],1).length!==2)process.exit(1)"], directory, environment)
    (directory / "package.json").write_text(json.dumps({"name": "pm-fixture", "private": True, "devDependencies": {"lodash": "4.17.20"}}))
    invoke([uf, "install"], directory, environment)
    _, prod = invoke([uf, "audit", "--prod", "--json"], directory, environment)
    assert json.loads(prod.stdout)["checked"] == 0
    print("native registry install, frozen reuse, JavaScript resolution and security audit passed", flush=True)


def benchmark(uf, directory, fixture, manager, repeat, environment):
    case = directory / fixture / str(repeat) / manager
    primary, second, cache = case / "primary", case / "second", case / "cache"
    project(primary, FIXTURES[fixture], manager)
    cache.mkdir(parents=True)
    env = environment.copy()
    if manager == "uf":
        env["UF_PM_STORE"] = str(cache)
        command = [uf, "install"]
        frozen = command + ["--frozen-lockfile"]
        lock = "uf.lock"
    elif manager == "pnpm":
        command = ["pnpm", "install", "--ignore-scripts", "--store-dir", str(cache)]
        frozen = command + ["--frozen-lockfile"]
        lock = "pnpm-lock.yaml"
    else:
        env["BUN_INSTALL_CACHE_DIR"] = str(cache)
        command = ["bun", "install", "--ignore-scripts"]
        frozen = command + ["--frozen-lockfile"]
        lock = "bun.lock"
    rows = []
    def measure(phase, project_dir, args):
        elapsed, _ = invoke(args, project_dir, env)
        row = {"fixture": fixture, "manager": manager, "repeat": repeat, "phase": phase, "milliseconds": elapsed}
        rows.append(row)
        print(json.dumps(row), flush=True)
    measure("cold", primary, command)
    measure("warm", primary, command)
    measure("frozen", primary, frozen)
    shutil.rmtree(primary / "node_modules")
    measure("reinstall", primary, frozen)
    project(second, FIXTURES[fixture], manager)
    shutil.copy2(primary / lock, second / lock)
    first_disk = disk_usage([cache, primary / "node_modules"])
    measure("second-project", second, frozen)
    final_disk = disk_usage([cache, primary / "node_modules", second / "node_modules"])
    # Check actual imports and executable wiring, rather than timing an empty tree.
    invoke(["node", "-e", "const r=require('react'); if(!r.createElement)process.exit(1)"], second, env)
    if fixture == "application":
        invoke(["node", "node_modules/vite/bin/vite.js", "--version"], second, env)
    return rows, {"fixture": fixture, "manager": manager, "repeat": repeat, "oneProject": first_disk, "twoProjects": final_disk,
                  "secondProjectAllocatedBytes": final_disk["allocatedBytes"] - first_disk["allocatedBytes"]}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--uf", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--repeat", type=int, default=3)
    args = parser.parse_args()
    uf = str(Path(args.uf).resolve())
    output = Path(args.output).resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    environment = dict(os.environ, CI="1", NO_COLOR="1")
    versions = {manager: subprocess.check_output([uf if manager == "uf" else manager, "--version"], env=environment, text=True).strip()
                for manager in ["uf", "pnpm", "bun", "node"]}
    assert versions["pnpm"].startswith("12."), versions
    rows, sizes = [], []
    with tempfile.TemporaryDirectory(prefix="uf-pm-benchmark-") as temporary:
        root = Path(temporary)
        environment.update(UF_PM_STORE=str(root / "smoke-store"), UF_STORE=str(root / "tool-store"), UF_ROOTS=str(root / "roots"))
        smoke(uf, root, environment)
        for fixture in FIXTURES:
            for repeat in range(args.repeat):
                managers = ["uf", "pnpm", "bun"]
                managers = managers[repeat % 3:] + managers[:repeat % 3]
                for manager in managers:
                    samples, allocated = benchmark(uf, root, fixture, manager, repeat, environment)
                    rows.extend(samples)
                    sizes.append(allocated)
                    output.write_text(json.dumps({"versions": versions, "fixtures": FIXTURES, "samples": rows, "disk": sizes}, indent=2) + "\n")
    grouped = {}
    for row in rows:
        key = f"{row['fixture']}/{row['manager']}/{row['phase']}"
        grouped.setdefault(key, []).append(row["milliseconds"])
    medians = {key: statistics.median(values) for key, values in grouped.items()}
    output.write_text(json.dumps({"versions": versions, "fixtures": FIXTURES, "samples": rows, "disk": sizes, "medianMilliseconds": medians,
        "method": "Three isolated cache repetitions, rotated manager order, identical direct versions and disabled install scripts; allocated bytes count each inode once across cache and projects."}, indent=2) + "\n")
    print(json.dumps(medians, indent=2))


if __name__ == "__main__":
    main()

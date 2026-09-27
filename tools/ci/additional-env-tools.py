#!/usr/bin/env python3
"""Exercise official native carriers and project tool selection."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

uf = str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory(prefix="uf-additional-tools-") as directory:
    root = Path(directory)
    project = root / "project"
    project.mkdir()
    (project / "package.json").write_text('{"name":"env-tools-smoke","private":true}')
    env = dict(os.environ, UF_STORE=str(root / "store"), UF_ENVS=str(root / "envs"), UF_ROOTS=str(root / "roots"))

    def run(*args):
        result = subprocess.run([uf, *args], cwd=project, env=env, text=True, capture_output=True, timeout=180)
        assert result.returncode == 0, (args, result.stdout, result.stderr)
        return result.stdout

    for tool, version, key in [("aube", "2.5.0", "packageManager"), ("nub", "0.9.5", "runtime")]:
        run("env", "use", f"{tool}@{version}")
        source = (project / "uf.config.js").read_text()
        assert key in source and f"{tool}@{version}" in source, source
        output = run("env", "exec", tool, "--version")
        assert version in output, output
    run("install")
    assert (project / "aube-lock.yaml").is_file()
    output = run("env", "exec", "nub", "-e", "console.log('runtime-ok')")
    assert "runtime-ok" in output, output
    print("official native carriers: aube and Nub project selection passed")

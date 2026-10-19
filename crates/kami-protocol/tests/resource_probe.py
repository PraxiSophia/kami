"""Run allocation measurements without changing the workspace's dependencies/lockfile.

Requires Cargo, network/cached allocation-counter 0.8.1, and an explicit scratch TMPDIR.
Only an isolated temporary package and its lockfile/target are created there.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    crate = Path(__file__).resolve().parents[1]
    scratch = Path(os.environ["TMPDIR"]).resolve()
    with tempfile.TemporaryDirectory(prefix="kami-protocol-alloc-", dir=scratch) as temp:
        root = Path(temp)
        manifest = "\n".join([
            '[package]', 'name = "kami-protocol-allocation-probe"',
            'version = "0.0.0"', 'edition = "2021"', '[workspace]',
            '[dependencies]', 'allocation-counter = "=0.8.1"',
            f'kami-protocol = {{ path = {json.dumps(str(crate))} }}',
            f'kami-core = {{ path = {json.dumps(str(crate.parent / "kami-core"))} }}',
            '[[bin]]', 'name = "probe"',
            f'path = {json.dumps(str(crate / "tests/allocation_probe/probe.rs"))}',
        ])
        (root / "Cargo.toml").write_text(manifest)
        result = subprocess.run([
            "cargo", "run", "--release", "--manifest-path", str(root / "Cargo.toml")
        ], env={**os.environ, "CARGO_TARGET_DIR": str(root / "target")}, check=False)
        raise SystemExit(result.returncode)


if __name__ == "__main__":
    main()

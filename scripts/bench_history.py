#!/usr/bin/env python3
"""合成履歴で CLI の検索時間と出力の一致を比較する。Python 3.7 以降で実行可能。"""

import argparse
import hashlib
import json
import os
import platform
import statistics
import subprocess
import tempfile
import time
from pathlib import Path


def main():
    """CLI を交互に計測し、生データと出力のハッシュを JSON に書く。"""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binaries", nargs="+", type=Path)
    parser.add_argument("--runs", type=int, default=15)
    args = parser.parse_args()
    if args.runs < 2:
        parser.error("--runs must be at least 2")
    binaries = [path.resolve(strict=True) for path in args.binaries]
    results = []
    with tempfile.TemporaryDirectory(prefix="zsh-turbo-bench-") as directory:
        root = Path(directory)
        env = dict(os.environ)
        for name in ("XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_STATE_HOME"):
            env[name] = str(root / name)
        cases = [
            ("small-prefix", 1000, "git ", "prefix", False),
            ("large-prefix", 100000, "git ", "prefix", False),
            ("large-prefix-miss", 100000, "missing ", "prefix", False),
            ("large-substring", 100000, "status", "substring", False),
            ("large-fuzzy", 100000, "gts", "fuzzy", False),
            ("large-miss", 100000, "ZZZ", "fuzzy", False),
            ("unicode-fuzzy", 100000, "猫犬", "fuzzy", True),
        ]
        for name, count, query, strategy, japanese in cases:
            history = root / "history"
            # Meta ペアを含む、実際の zsh と同じ形式を使う。
            text = "".join(
                ": 1700000000:0;git -C project-{:06d} status {}\n".format(
                    index % 10000, "猫と犬" if japanese else "--short"
                )
                for index in range(count)
            ).encode("utf-8")
            encoded = bytearray()
            for byte in text:
                if byte >= 128:
                    encoded.extend((0x83, byte ^ 0x20))
                else:
                    encoded.append(byte)
            history.write_bytes(encoded)
            samples = [[] for _ in binaries]
            expected = None
            # 同じケースのバイナリを交互に走らせ、先に3回ずつウォームアップする。
            for run in range(args.runs + 3):
                for index, binary in enumerate(binaries):
                    command = [
                        str(binary),
                        "complete",
                        "--history-file",
                        str(history),
                        "--strategy",
                        strategy,
                        "--max",
                        "20",
                        "--",
                        query,
                    ]
                    started = time.perf_counter_ns()
                    output = subprocess.check_output(command, cwd=root, env=env)
                    elapsed = (time.perf_counter_ns() - started) / 1000000
                    if expected is None:
                        expected = output
                    if output != expected:
                        raise RuntimeError(f"output differs: {name} ({binary.name})")
                    if run >= 3:
                        samples[index].append(elapsed)
            for index, binary in enumerate(binaries):
                timings = samples[index]
                results.append(
                    {
                        "case": name,
                        "binary": binary.name,
                        "entries": count,
                        "bytes": len(encoded),
                        "query": query,
                        "strategy": strategy,
                        "samples_ms": timings,
                        "median_ms": statistics.median(timings),
                        "stdev_ms": statistics.stdev(timings),
                        "output_sha256": hashlib.sha256(expected).hexdigest(),
                    }
                )
    print(
        json.dumps(
            {
                "system": platform.system(),
                "machine": platform.machine(),
                "python": platform.python_version(),
                "warmups": 3,
                "runs": args.runs,
                "results": results,
            },
            ensure_ascii=False,
            indent=2,
        )
    )


if __name__ == "__main__":
    main()

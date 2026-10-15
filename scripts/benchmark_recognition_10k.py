"""评估固定万条语料的完整字段命中率，保存错误分组与批量耗时。"""

import argparse
import gzip
import hashlib
import importlib.metadata
import json
import statistics
import sys
import time
from collections import Counter, defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import moviepilot_rust

from tests.metainfo.options import build_options


def main() -> None:
    """复用同一语料比较不同安装版本；失败保留输入和字段差异供审查。"""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--loops", type=int, default=3)
    parser.add_argument("--repeats", type=int, default=5)
    args = parser.parse_args()
    if min(args.loops, args.repeats) < 1:
        parser.error("loops 和 repeats 必须大于零")
    payload = gzip.decompress((Path(__file__).resolve().parents[1] / "tests/metainfo/recognition_10k.jsonl.gz").read_bytes())
    rows = [json.loads(line) for line in payload.splitlines()]
    options = build_options(["#"])

    def parse(row):
        if row["entrypoint"] == "path":
            return moviepilot_rust.parse_metainfo_path_fast(row["title"], options)
        return moviepilot_rust.parse_metainfo_fast(row["title"], row.get("subtitle"), options)

    groups = {key: defaultdict(lambda: {"passed": 0, "total": 0})
              for key in ("origin", "source", "transformation", "entrypoint")}
    failures = []
    fields = Counter()
    for row in rows:
        actual = parse(row)
        differences = {key: {"expected": value, "actual": actual.get(key)}
                       for key, value in row["expected"].items() if actual.get(key) != value}
        for key, values in groups.items():
            values[row[key]]["total"] += 1
            values[row[key]]["passed"] += not differences
        if differences:
            failures.append({**row, "differences": differences})
            fields.update(differences.keys())
    timings = []
    for _ in range(args.repeats):
        start = time.perf_counter()
        for _ in range(args.loops):
            for row in rows:
                parse(row)
        timings.append((time.perf_counter() - start) * 1000 / args.loops)
    report = {
        "version": importlib.metadata.version("moviepilot-rust"),
        "sha256_uncompressed": hashlib.sha256(payload).hexdigest(),
        "total": len(rows), "passed": len(rows) - len(failures),
        "groups": groups, "failed_fields": fields,
        "median_ms": statistics.median(timings), "timings_ms": timings,
        "loops": args.loops, "repeats": args.repeats,
    }
    args.output.write_text(json.dumps({**report, "failures": failures}, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(report, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()

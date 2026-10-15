"""离线评估人工标注样本的正确性，并测量完整站点语料的 Rust 解析耗时。"""

import argparse
import json
import statistics
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import moviepilot_rust

from tests.metainfo.options import build_options


def main():
    """输出可比较的 JSON；兼容快照和人工标注分别计数。"""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--loops", type=int, default=10)
    parser.add_argument("--repeats", type=int, default=7)
    args = parser.parse_args()
    if args.loops < 1 or args.repeats < 1:
        parser.error("loops 和 repeats 必须大于零")
    corpus_dir = Path(__file__).resolve().parents[1] / "tests/metainfo"
    samples = [
        sample
        for filename in ("site_samples.json", "expanded_samples.json")
        for sample in json.loads((corpus_dir / filename).read_text())["samples"]
    ]
    options = build_options(["#"])
    results = {"annotated": [0, 0], "compatibility": [0, 0]}
    failures = []
    for sample in samples:
        parsed = moviepilot_rust.parse_metainfo_fast(sample["title"], sample.get("subtitle"), options)
        if "expected" not in sample:
            continue
        category = sample.get("expectation", "annotated")
        results[category][1] += 1
        if all(parsed.get(key) == value for key, value in sample["expected"].items()):
            results[category][0] += 1
        else:
            failures.append(sample["id"])
    timings = []
    for _ in range(args.repeats):
        start = time.perf_counter()
        for _ in range(args.loops):
            for sample in samples:
                moviepilot_rust.parse_metainfo_fast(sample["title"], sample.get("subtitle"), options)
        timings.append((time.perf_counter() - start) * 1000 / args.loops)
    print(json.dumps({
        "samples": len(samples), "passed_total": results, "failures": failures,
        "median_ms": statistics.median(timings), "timings_ms": timings,
    }, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()

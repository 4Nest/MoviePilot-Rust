"""构建副标题专项语料：独立语义期望与真实站点记录，不调用解析器。"""

import gzip
import hashlib
import json
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / "tests" / "metainfo"


def main() -> None:
    """冻结一万条主副标题组合，保留来源、变换规则和完整校验和。"""
    rows = json.loads((ROOT / "subtitle_samples.json").read_text())["samples"]
    original = [json.loads(line) for line in gzip.decompress(
        (ROOT / "recognition_10k.jsonl.gz").read_bytes()
    ).splitlines()]
    # 这里只添加不描述本资源季集的说明；原独立标注应当保持不变。
    notes = (
        "备注：前季第01-99集合集另见；本条保持原发布编号",
        "剧情：主角在第三季比赛中回顾第十二集发生的故事",
        "简介：Episode 99 是主角正在寻找的线索",
        "备注：预计共24集，每周更新2集，下周更新第03集",
        "备注：原始画质请以标题为准；2160p HDR 10bit HEVC DTS-HD MA 7.1",
        "备注：简中 / 繁中 / CHS&CHT / 中日双语 / 内封字幕",
    )
    seen = {(row["title"], row["subtitle"]) for row in rows}
    for round_index in range(len(notes)):
        for seed_index, seed in enumerate(original):
            if seed["entrypoint"] != "title":
                continue
            note_index = (seed_index + round_index) % len(notes)
            note = notes[note_index]
            subtitle = f"{seed['subtitle']} | {note}" if seed.get("subtitle") else note
            key = seed["title"], subtitle
            if key in seen:
                continue
            seen.add(key)
            rows.append({
                "id": f"subtitle-{seed['id']}-{note_index}", "source": seed["source"],
                "origin": "derived", "category": "description-isolation",
                "seed_id": seed["id"], "title": seed["title"], "subtitle": subtitle,
                "expected": seed["expected"], "python_compatible": False,
            })
            if len(rows) == 10_000:
                break
        if len(rows) == 10_000:
            break
    assert len(rows) == len(seen) == 10_000
    assert len({row["id"] for row in rows}) == 10_000
    payload = "".join(json.dumps(row, ensure_ascii=False, separators=(",", ":")) + "\n"
                      for row in rows).encode()
    (ROOT / "subtitle_10k.jsonl.gz").write_bytes(gzip.compress(payload, mtime=0))
    manifest = {
        "records": len(rows), "sha256_uncompressed": hashlib.sha256(payload).hexdigest(),
        "origin": dict(Counter(row["origin"] for row in rows)),
        "source": dict(Counter(row["source"] for row in rows)),
        "category": dict(Counter(row["category"] for row in rows)),
        "field_assertions": sum(len(row["expected"]) for row in rows),
        "annotation": "真实主副标题独立标注；构造案例按语义指定期望；说明变体继承已有独立标注。不是一万条新抓取副标题，也不是全站准确率。",
    }
    (ROOT / "subtitle_10k_manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(manifest, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()

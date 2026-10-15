"""构建固定一万条独立期望回归；只读取标注，不调用解析器。"""

import gzip
import hashlib
import json
import re
from collections import Counter, defaultdict, deque
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CORPUS = ROOT / "tests" / "metainfo"
COUNT = 10_000
CORE_FIELDS = {
    "cn_name", "en_name", "year", "begin_season", "end_season", "total_season",
    "begin_episode", "end_episode", "total_episode", "resource_pix", "isfile",
    "video_encode", "video_bit", "resource_effect", "fps", "type",
}


def _key(row: dict) -> tuple:
    return row.get("entrypoint", "title"), row["title"], row.get("subtitle")


def _variant(seed: dict, operation: str, title: str, **expected_changes) -> dict:
    row = dict(seed, id=f"{seed['id']}--{operation}", title=title, origin="derived",
               seed_id=seed["id"], transformation=operation.split(":", 1)[0])
    row["expected"] = {**seed["expected"], **expected_changes}
    return row


def _stem_suffix(title: str) -> tuple[str, str]:
    for suffix in (".mkv", ".mp4", ".ass", ".srt", ".ssa", ".m2ts", ".avi"):
        if title.lower().endswith(suffix):
            return title[:-len(suffix)], title[-len(suffix):]
    return title, ""


def _variants(seed: dict):
    title = seed["title"]
    stem, suffix = _stem_suffix(title)
    for pix in ("720p", "1080p", "2160p"):
        changed = re.sub(r"(?i)(?<!\d)(?:720|1080|2160)p(?!\d)", pix, title)
        if changed != title:
            yield _variant(seed, f"resolution:{pix}", changed, resource_pix=pix)
    for label in ("简中", "繁日內嵌", "CHS&CHT", "简繁日内封字幕"):
        yield _variant(seed, f"subtitle-label:{label}", f"{stem}[{label}]{suffix}")
        group = re.match(r"^(?:\[[^\]]+\]|【[^】]+】)", title)
        if group:
            changed = title[:group.end()] + f"[{label}]" + title[group.end():]
            yield _variant(seed, f"leading-label:{label}", changed)
    for opening, closing, direction in (("【", "】", "wide"), ("[", "]", "ascii")):
        changed = title.translate(str.maketrans({"[": opening, "]": closing, "【": opening, "】": closing}))
        if changed != title:
            expected = {k: v.translate(str.maketrans({"[": opening, "]": closing, "【": opening, "】": closing}))
                        for k, v in seed["expected"].items() if k in ("cn_name", "en_name") and isinstance(v, str)}
            yield _variant(seed, f"brackets:{direction}", changed, **expected)
    if seed["source"] != "audiences":
        for name in ("Example-Raws", "测试发布组"):
            changed = re.sub(r"^(?:\[[^\]]+\]|【[^】]+】)", f"[{name}]", title, count=1)
            if changed != title:
                yield _variant(seed, f"release-group:{name}", changed)
    # 只修改与人工标注首集相符的明确标记，避免把电影序号、年份当成集号变换。
    episode = seed["expected"].get("begin_episode")
    if episode is not None:
        for revision in (2, 12):
            for match in re.finditer(r"[\[【](\d{1,4})[\]】]", title):
                if int(match[1]) == episode:
                    changed = title[:match.end()-1] + f"v{revision}" + title[match.end()-1:]
                    yield _variant(seed, f"episode-revision:{revision}", changed)
                    break
    if suffix:
        for ext in (".mkv", ".mp4", ".chs.ass", ".zh-Hant.srt"):
            # 已有字幕语言尾缀不叠加到视频文件名中。
            clean = re.sub(r"(?i)(?:[.](?:chs|cht|sc|tc|zh-hant|zh-hans))+$", "", stem)
            changed = clean + ext
            if changed != title:
                yield _variant(seed, f"extension:{ext}", changed, isfile=True)
        for parent in ("/library/电影与动画", "/media/Anime Archive"):
            if "/" not in title:
                row = _variant(seed, f"path:{parent}", f"{parent}/{title}")
                row["entrypoint"] = "path"
                yield row
    # 复制粘贴的边缘空白不得改变名称和文件后缀判定。
    yield _variant(seed, "outer-whitespace:space", f"  {title}  ")
    yield _variant(seed, "outer-whitespace:unicode", f"\u3000{title}\u00a0")


def main() -> None:
    """合并独立标注、均衡作品与变换类型，并输出可审查的固定语料和清单。"""
    unique = {}
    for filename in ("site_samples.json", "expanded_samples.json", "volume_samples.json", "diverse_samples.json"):
        for source in json.loads((CORPUS / filename).read_text(encoding="utf-8"))["samples"]:
            if "expected" not in source or source.get("expectation") == "compatibility":
                continue
            row = {k: source[k] for k in ("id", "source", "title", "subtitle", "url", "kind") if k in source}
            row.update(origin="real", entrypoint="title", transformation="original")
            row["expected"] = {k: v for k, v in source["expected"].items() if k in CORE_FIELDS}
            if _key(row) in unique:
                existing = unique[_key(row)]["expected"]
                assert all(k not in existing or existing[k] == v for k, v in row["expected"].items())
                existing.update(row["expected"])
            else:
                unique[_key(row)] = row
    seeds = list(unique.values())
    # 每个作品和变换类别轮流出样，避免某部番剧的大量字幕文件占满派生用例。
    groups = defaultdict(deque)
    for seed in seeds:
        name = seed["expected"].get("cn_name") or seed["expected"].get("en_name") or seed["id"]
        for row in _variants(seed):
            groups[(name, row["transformation"])].append(row)
    queues = [groups[key] for key in sorted(groups)]
    while True:
        progressed = False
        for queue in queues:
            if not queue:
                continue
            row = queue.popleft()
            if _key(row) not in unique:
                unique[_key(row)] = row
                progressed = True
            if len(unique) == COUNT:
                break
        if len(unique) == COUNT:
            break
        if not progressed and not any(queues):
            raise ValueError(f"只有 {len(unique)} 条不同输入，不能重复凑满一万条")
    rows = list(unique.values())
    assert len(rows) == COUNT and len({_key(row) for row in rows}) == COUNT
    assert len({row["id"] for row in rows}) == COUNT
    payload = "".join(json.dumps(row, ensure_ascii=False, separators=(",", ":")) + "\n" for row in rows).encode()
    target = CORPUS / "recognition_10k.jsonl.gz"
    target.write_bytes(gzip.compress(payload, mtime=0))
    manifest = {
        "records": len(rows), "unique_inputs": len(unique),
        "sha256_uncompressed": hashlib.sha256(payload).hexdigest(),
        "origin": dict(Counter(row["origin"] for row in rows)),
        "source": dict(Counter(row["source"] for row in rows)),
        "entrypoint": dict(Counter(row["entrypoint"] for row in rows)),
        "transformations": dict(Counter(row["transformation"] for row in rows)),
        "seed_names": len({row["expected"].get("cn_name") or row["expected"].get("en_name") for row in seeds}),
        "annotation": "原始期望来自独立标注；派生输入只作列明的语义保持变换，不调用被测解析器。",
    }
    (CORPUS / "recognition_10k_manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2)+"\n", encoding="utf-8")
    print(json.dumps(manifest, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()

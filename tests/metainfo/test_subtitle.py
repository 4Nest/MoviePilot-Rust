"""副标题补充、干扰和冲突回归，覆盖真实 PT 记录及构造边界。"""

import gzip
import hashlib
import json
from pathlib import Path

import moviepilot_rust
import pytest

from tests.metainfo.options import build_options

ROOT = Path(__file__).parent
PAYLOAD = gzip.decompress((ROOT / "subtitle_10k.jsonl.gz").read_bytes())
SAMPLES = [json.loads(line) for line in PAYLOAD.splitlines()]


def test_subtitle_corpus_integrity() -> None:
    """校验独立输入、条数和摘要，防止测试语料静默漂移。"""
    manifest = json.loads((ROOT / "subtitle_10k_manifest.json").read_text())
    assert len(SAMPLES) == manifest["records"] == 10_000
    assert len({(row["title"], row["subtitle"]) for row in SAMPLES}) == 10_000
    assert hashlib.sha256(PAYLOAD).hexdigest() == manifest["sha256_uncompressed"]


@pytest.mark.parametrize("sample", SAMPLES, ids=lambda row: row["id"])
def test_subtitle_recognition(sample: dict) -> None:
    """主标题确定字段保持优先，副标题只补充本资源的有效信息。"""
    parsed = moviepilot_rust.parse_metainfo_fast(
        sample["title"], sample["subtitle"], build_options(["#"])
    )
    assert {key: parsed.get(key) for key in sample["expected"]} == sample["expected"]
    assert parsed["subtitle"] == sample["subtitle"].strip()

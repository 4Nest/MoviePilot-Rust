"""固定一万条输入的离线回归，期望来自独立标注及语义保持变换。"""

import gzip
import hashlib
import json
from pathlib import Path

import moviepilot_rust
import pytest

from tests.metainfo.options import build_options

CORPUS = Path(__file__).with_name("recognition_10k.jsonl.gz")
PAYLOAD = gzip.decompress(CORPUS.read_bytes())
SAMPLES = [json.loads(line) for line in PAYLOAD.splitlines()]


def test_corpus_is_frozen_and_unique():
    """数量、唯一输入、独立期望与校验和共同防止重复凑数或静默修改。"""
    manifest = json.loads(CORPUS.with_name("recognition_10k_manifest.json").read_text())
    assert len(SAMPLES) == manifest["records"] == 10_000
    assert len({(s["entrypoint"], s["title"], s.get("subtitle")) for s in SAMPLES}) == 10_000
    assert len({s["id"] for s in SAMPLES}) == 10_000
    assert all(s["expected"] for s in SAMPLES)
    assert hashlib.sha256(PAYLOAD).hexdigest() == manifest["sha256_uncompressed"]


@pytest.mark.parametrize("sample", SAMPLES, ids=lambda s: s["id"])
def test_recognition_10k(sample):
    """标题和路径分别走对应 Rust 入口，逐字段验证固定期望。"""
    options = build_options(["#"])
    if sample["entrypoint"] == "path":
        actual = moviepilot_rust.parse_metainfo_path_fast(sample["title"], options)
    else:
        actual = moviepilot_rust.parse_metainfo_fast(sample["title"], sample.get("subtitle"), options)
    assert {key: actual.get(key) for key in sample["expected"]} == sample["expected"]

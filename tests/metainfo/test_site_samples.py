"""离线复放 BT 真实发布名、文件名及 PT 兼容样本，不访问站点。"""

import json
from pathlib import Path

import moviepilot_rust
import pytest

from tests.metainfo.options import build_options

SAMPLES = [
    sample
    for filename in ("site_samples.json", "expanded_samples.json")
    for sample in json.loads(Path(__file__).with_name(filename).read_text())["samples"]
]


def test_tenipuri_chapter_preserves_series_aliases():
    """篇章名交给媒体库匹配，U-17 和英文简称不能误当季集数字。"""
    title = (
        "[黒ネズミたち] 新网球王子 U-17 WORLD CUP 世界杯决赛成员决定战 / "
        "Tenipuri: U-17 Kesshou Member Ketteisen - 03 (Baha 1920x1080 AVC AAC MP4)"
    )
    parsed = moviepilot_rust.parse_metainfo_fast(title, None, build_options(["#"]))
    assert parsed["cn_name"] == "新网球王子 U-17 WORLD CUP 世界杯决赛成员决定战"
    assert parsed["en_name"] == "Tenipuri: U-17 Kesshou Member Ketteisen"
    assert parsed["begin_season"] is None
    assert parsed["begin_episode"] == 3
    assert parsed["resource_pix"] == "1080p"


@pytest.mark.parametrize("sample", [s for s in SAMPLES if "expected" in s], ids=lambda s: s["id"])
def test_site_sample(sample):
    """独立标注验证 BT 关键字段，PT 快照保护已有识别结果。"""
    parsed = moviepilot_rust.parse_metainfo_fast(
        sample["title"], sample.get("subtitle"), build_options(["#"])
    )
    assert {key: parsed.get(key) for key in sample["expected"]} == sample["expected"]


@pytest.mark.parametrize("label", ["简中", "繁中", "简日内嵌", "繁日內嵌", "简繁内封", "CHS&CHT", "GB", "MP4"])
def test_language_labels_never_replace_title(label):
    """标签顺序、闭括号和字幕语言不能影响已确认的中英文片名。"""
    for tail in (f"[{label}]", f"[{label}][MP4]"):
        title = f"[字幕组][测试动画][Test Anime][13][1080p]{tail}"
        parsed = moviepilot_rust.parse_metainfo_fast(title, None, build_options())
        assert parsed["cn_name"] == "测试动画"
        assert parsed["en_name"] == "Test Anime"
        assert parsed["begin_episode"] == 13


@pytest.mark.parametrize("episode", ["[01v2]", "[01]v2", "[01][v2]", "- 01 v3", "S03E01v2"])
def test_revision_keeps_episode_and_original_title(episode):
    """修订号不是集号，原文仍可供显示、识别词和后续处理使用。"""
    title = f"[Group] 测试动画 第三季 / Test Anime S3 {episode} [1080p][简中]"
    parsed = moviepilot_rust.parse_metainfo_fast(title, None, build_options())
    assert parsed["begin_episode"] == 1
    assert parsed["begin_season"] == 3
    assert parsed["org_string"] == title


def test_custom_words_and_parent_directory():
    """别名恢复使用识别词处理后的输入，裸修订版文件从父目录继承名称。"""
    title = "[Group] 旧名 / Old Name [01v2][1080p][简中]"
    words = ["旧名 => 新名", "Old Name => New Name"]
    parsed = moviepilot_rust.parse_metainfo_fast(title, None, build_options(words))
    assert parsed["cn_name"] == "新名"
    assert parsed["en_name"] == "New Name"
    assert parsed["apply_words"] == words
    assert parsed["original_name"] == "旧名"
    parsed = moviepilot_rust.parse_metainfo_path_fast(
        "/anime/测试动画 第三季/01.v3.mkv", build_options()
    )
    assert parsed["cn_name"] == "测试动画"
    assert parsed["begin_season"] == 3
    assert parsed["begin_episode"] == 1


@pytest.mark.parametrize("title", [
    "Kokuho 2025 REPACK2 1080p AMZN WEB-DL DDP5.1 H.264-Group",
    "V2 Escape from Hell 2021 1080p BluRay x265-Group",
    "The Lost National Treasure S01 2025 V2 2160p WEB-DL H265 AAC-ADWeb",
])
def test_pt_revision_is_not_episode(title):
    """PT 片名和版本中的数字不能变成动漫集号。"""
    parsed = moviepilot_rust.parse_metainfo_fast(title, None, build_options())
    assert parsed["begin_episode"] is None
    assert parsed["end_episode"] is None


@pytest.mark.parametrize("opening,closing", [("[", "]"), ("【", "】")])
def test_inline_title_brackets_and_leading_labels(opening, closing):
    """强调括号保留在片名中，前置字幕标签和后置季集仍独立识别。"""
    cn_name = f"勇者{opening}防御{closing}日记"
    en_name = f"Hero {opening}Guard{closing} Diary"
    title = f"[Group][CHS&CHT] {cn_name} / {en_name} [03v2][1080p].mkv"
    parsed = moviepilot_rust.parse_metainfo_fast(title, None, build_options())
    assert parsed["cn_name"] == cn_name
    assert parsed["en_name"] == en_name
    assert parsed["begin_episode"] == 3
    assert parsed["title"] == title


@pytest.mark.parametrize("padding", ["  ", "\u3000", "\u00a0"])
def test_padded_filename_keeps_suffix_and_bonus_classification(padding):
    """复制粘贴产生的边缘空白不影响文件与特典判定，显示标题保留原文。"""
    title = f"{padding}[Group] Example [NCOP01][Ma10p_1080p].mkv{padding}"
    parsed = moviepilot_rust.parse_metainfo_fast(title, None, build_options())
    assert parsed["isfile"] is True
    assert parsed["en_name"] == "Example"
    assert parsed["begin_episode"] is None
    assert parsed["title"] == title


def test_leading_labels_preserve_original_string():
    """仅在解析时清理字幕标签，识别词和界面所用的原文仍完整保留。"""
    title = "[Group][简中][CHS&CHT] Example Movie (2026) 1080p WEB-DL"
    parsed = moviepilot_rust.parse_metainfo_fast(title, None, build_options())
    assert parsed["cn_name"] is None
    assert parsed["en_name"] == "Example Movie"
    assert parsed["org_string"] == title


@pytest.mark.parametrize("source", ["BDRip", "BDRemux", "BluRay", "WEB-DL", "WEBRip"])
def test_source_after_subtitle_label_is_not_an_alias(source):
    """字幕标签后的裸片源属于技术参数，不能覆盖已提取的英文别名。"""
    title = f"[Group] 示例电影 / Example Movie 2026 [简繁字幕] {source} 1080p x265"
    parsed = moviepilot_rust.parse_metainfo_fast(title, None, build_options())
    assert parsed["cn_name"] == "示例电影"
    assert parsed["en_name"] == "Example Movie"
    assert parsed["year"] == "2026"
    assert parsed["begin_episode"] is None

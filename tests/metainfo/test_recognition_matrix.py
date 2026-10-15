"""确定性组合回归：期望由输入语义给出，不从解析结果生成。"""

from itertools import product as _product

import moviepilot_rust
import pytest

from tests.metainfo.options import build_options

LABELS = (
    "简中", "繁中", "简日内嵌", "繁日內嵌", "简繁内封", "简体双语",
    "CHS", "CHT", "CHS&CHT", "GB", "BIG5", "中日双语", "简繁日内封字幕", "MP4",
)
NAMES = (
    ("星空旅人", "Star Travelers", "Star Travelers"),
    ("不存在的动画", "An Imaginary Anime", "An Imaginary Anime"),
    ("新网球王子 U-17 世界杯", "The Prince of Tennis II", "The Prince Of Tennis Ii"),
    ("86 不存在的战区", "86 Eighty-Six", "86 Eighty-Six"),
)


def product(*args):
    """固定参数矩阵，便于 pytest 收集与报告。"""
    return list(_product(*args))


def _parse(title: str, words=None) -> dict:
    return moviepilot_rust.parse_metainfo_fast(title, None, build_options(words))


@pytest.mark.parametrize("label,brackets,placement,names", product(
    LABELS, ("square", "wide"), ("front", "middle", "tail"), NAMES
))
def test_subtitle_labels_and_aliases(label, brackets, placement, names) -> None:
    """交换字幕标签位置和括号，不应改变中英文名及集号。"""
    cn, en, expected_en = names
    tags = {"front": f"[{label}][{cn}][{en}][07]",
            "middle": f"[{cn}][{label}][{en}][07]",
            "tail": f"[{cn}][{en}][07][{label}]"}
    title = f"[ExampleGroup]{tags[placement]}[1080p][AVC][AAC]"
    if brackets == "wide":
        title = title.replace("[", "【").replace("]", "】")
    parsed = _parse(title)
    assert (parsed["cn_name"], parsed["en_name"], parsed["begin_episode"]) == (cn, expected_en, 7)
    assert parsed["resource_pix"] == "1080p"


@pytest.mark.parametrize("season,episode,revision,extension", product(
    (0, 1, 3, 12), (1, 9, 24, 105, 1200), ("v2", "v3", "v12"), ("", ".mkv", ".chs.ass")
))
def test_revision_and_file_extensions(season, episode, revision, extension) -> None:
    """多位修订号和字幕后缀不应污染显式季集，S00 必须保留。"""
    title = f"[ExampleGroup] 测试动画 / Example Anime S{season:02}E{episode:02}{revision} [1080p][CHS]{extension}"
    parsed = _parse(title)
    assert (parsed["cn_name"], parsed["en_name"]) == ("测试动画", "Example Anime")
    assert (parsed["begin_season"], parsed["end_season"]) == (season, None)
    assert (parsed["begin_episode"], parsed["end_episode"], parsed["total_episode"]) == (episode, None, 1)
    assert parsed["isfile"] is bool(extension)


@pytest.mark.parametrize("episode,reference,codec", product(
    (1, 2, 6, 8, 12), ("S01E00", "S01E17.5", "S02E13", "S03E25"), ("AVC", "HEVC", "AV1")
))
def test_specials_do_not_merge_reference_numbers(episode, reference, codec) -> None:
    """特典主编号不能与末尾正篇对照合并成跨季范围。"""
    title = f"[Group] Example Anime S00E{episode:02} (BD 1080p {codec} FLAC) | OVA ({reference})"
    parsed = _parse(title)
    assert (parsed["begin_season"], parsed["end_season"], parsed["total_season"]) == (0, None, 1)
    assert (parsed["begin_episode"], parsed["end_episode"], parsed["total_episode"]) == (episode, None, 1)


@pytest.mark.parametrize("begin,end,separator,label", product(
    (1, 13, 67), (85, 120), ("-", "~", "～", "–"), ("合集", "精修合集", "修正合集", "END")
))
def test_batch_ranges(begin, end, separator, label) -> None:
    """起始集不是 01 的合集与 Unicode 范围符号应得到相同区间。"""
    title = f"[Group][测试动画 / Example Anime][{begin:02}{separator}{end} {label}][1080p][简中]"
    parsed = _parse(title)
    assert (parsed["cn_name"], parsed["en_name"]) == ("测试动画", "Example Anime")
    assert (parsed["begin_episode"], parsed["end_episode"], parsed["total_episode"]) == (begin, end, end-begin+1)


@pytest.mark.parametrize("number,codec,season", product(
    (86, 2049, 2077, 2099), ("HEVC", "AVC", "AV1"), ("S1", "S1-S3", "S2-S4")
))
def test_numeric_titles_and_bare_technical_suffix(number, codec, season) -> None:
    """双语片名中的数字不是年份，位深不是集号，尾部合集季号单独处理。"""
    parsed = _parse(f"[Group] 星际{number} / Galaxy {number} 10-bit 1080p {codec} BDRip [{season} Fin]")
    assert (parsed["cn_name"], parsed["en_name"], parsed["year"]) == (f"星际{number}", f"Galaxy {number}", None)
    assert (parsed["begin_episode"], parsed["end_episode"]) == (None, None)
    expected = {"S1": (1, None, 1), "S1-S3": (1, 3, 3), "S2-S4": (2, 4, 3)}[season]
    assert (parsed["begin_season"], parsed["end_season"], parsed["total_season"]) == expected


@pytest.mark.parametrize("name,year,pix,source,codec", product(
    ("Silent River", "Northern Lights", "V2 Escape from Hell", "The Paper Tigers"),
    (1999, 2020, 2026), ("720p", "1080p", "2160p"), ("WEB-DL", "BluRay"), ("x264", "x265", "AV1")
))
def test_pt_movie_technical_combinations(name, year, pix, source, codec) -> None:
    """PT 电影的年份、分辨率和编码应独立识别，不产生伪季集。"""
    parsed = _parse(f"{name} {year} {pix} {source} {codec} DTS-HD MA 5.1-ADWeb")
    assert parsed["en_name"] == name.title()
    assert parsed["year"] == str(year)
    assert (parsed["resource_pix"], parsed["video_encode"]) == (pix, codec)
    assert (parsed["begin_episode"], parsed["end_episode"]) == (None, None)


@pytest.mark.parametrize("season,episode,extension,revision", product(
    (1, 2, 12), (1, 12, 105), ("mkv", "mp4", "chs.ass"), ("", ".v2")
))
def test_parent_directory_inheritance(season, episode, extension, revision) -> None:
    """裸编号文件继承父目录名称与季号，字幕和修订版使用同一语义。"""
    parsed = moviepilot_rust.parse_metainfo_path_fast(
        f"/library/测试动画/Season {season:02}/{episode:02}{revision}.{extension}", build_options()
    )
    assert parsed["cn_name"] == "测试动画"
    assert (parsed["begin_season"], parsed["begin_episode"]) == (season, episode)


@pytest.mark.parametrize("season,episode,tmdbid", product((0, 2, 10), (1, 12, 99), (12345, 67890)))
def test_explicit_metadata_overrides_detected_numbers(season, episode, tmdbid) -> None:
    """用户显式标签与识别词始终优先于站点原始季集。"""
    title = f"[Group] 旧名称 / Old Name S01E01v2 [1080p] {{[tmdbid={tmdbid};type=tv;s={season};e={episode}]}}"
    parsed = _parse(title, ["旧名称 => 新名称", "Old Name => New Name"])
    assert (parsed["cn_name"], parsed["en_name"]) == ("新名称", "New Name")
    assert (parsed["tmdbid"], parsed["begin_season"], parsed["begin_episode"]) == (tmdbid, season, episode)

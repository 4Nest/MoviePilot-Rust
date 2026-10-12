"""验证大词表缓存通过公开解析入口保持识别语义和配置隔离。"""

import moviepilot_rust

from tests.metainfo.options import build_options


def test_large_word_list_keeps_rule_order_and_changed_options():
    """超过旧正则容量的词表跨标题复用，并立即应用新词表与顺序。"""
    words = [f"UnusedRule{i}x => Ignored{i}x" for i in range(1200)]
    words += ["旧名 => 新名 && 第 <> 集 >> EP+1", "新名 => 新剧", "屏蔽"]
    options = build_options(custom_words=words)
    for _ in range(3):
        parsed = moviepilot_rust.parse_metainfo_fast("旧名 屏蔽", "第02集", options)
        assert parsed["cn_name"] == "新剧"
        assert parsed["begin_episode"] == 3
        assert parsed["apply_words"] == words[-3:]
    changed = build_options(custom_words=words[:-3] + ["旧名 => 另一部"])
    parsed = moviepilot_rust.parse_metainfo_fast("旧名", None, changed)
    assert parsed["cn_name"] == "另一部"
    parsed = moviepilot_rust.parse_metainfo_fast("旧名 屏蔽", "第02集", options)
    assert parsed["cn_name"] == "新剧"
    assert parsed["begin_episode"] == 3

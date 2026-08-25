"""共用 exclusion 層——profile ``exclude`` 單一源（報告 §8 薄層清單 #4）。

排除對象＝repo 擁有的噪音實證（mosaic：R1 stubs 假邊、R3b ai-analysis/
_archive 巨型資料檔霸榜）；generic fallback 僅 ``.venv/``。snapshot/
hub_refs import 同一入口；transition 消費的 snapshot 已在導出時過濾
（經 S2 間接生效，不重複 import）——禁各自維護副本。前綴一律目錄粒度
（帶斜線）——無斜線條目會誤傷同名開頭檔（``.venv-setup.py``）。
"""

from code_reality.profile import DEFAULT_EXCLUDE, Profile


def is_excluded(rel_path: str, profile: Profile | None) -> bool:
    """repo 相對路徑是否命中排除前綴（profile.exclude；無 profile → .venv/）。"""
    prefixes = profile.exclude if profile is not None else DEFAULT_EXCLUDE
    return any(rel_path.startswith(prefix) for prefix in prefixes)

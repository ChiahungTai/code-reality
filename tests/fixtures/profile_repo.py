"""測試 helper——合成 repo 的 .code-reality.toml 寫入（profile 引擎化隨遷）。

mosaic/NT 兩形狀與真 repo profile 檔同值——測試合成 repo 行為與真 repo
一致（無 profile 的 generic 行為另由 test_profile 直接構造 Profile 測）。
"""

from pathlib import Path

MOSAIC_PROFILE = """\
exclude = ["stubs/", "ai-analysis/", ".venv/", "snapshot/"]

[[module]]
prefix = "mosaic_alpha/"
depth = 1
"""

NT_PROFILE = """\
[[module]]
prefix = "crates/"
depth = 1

[[scan_root]]
path = "crates/**/*.rs"
pyi = "python/nautilus_trader/**/*.pyi"
"""


def write_mosaic_profile(repo: Path) -> Path:
    path = repo / ".code-reality.toml"
    path.write_text(MOSAIC_PROFILE)
    return path


def write_nt_profile(repo: Path) -> Path:
    path = repo / ".code-reality.toml"
    path.write_text(NT_PROFILE)
    return path

"""repo profile——``.code-reality.toml`` 單一源。

repo 事實歸 repo：module 規則（``module_of``）、claims 前綴衍生、
exclusions、boundary 掃描根全由 profile 檔擁有；工具層不內建任何
repo 特例（跨域違規五條的參數化落點——EP ep-code-reality-extraction）。

無 profile 的 generic fallback：module＝頂層目錄、exclude＝``.venv/``、
claims 不命中、boundary 掃描根空（caller crash-only 要求顯式 ``--repo``
＋profile）。
"""

import re
import tomllib
from dataclasses import dataclass
from pathlib import Path

PROFILE_FILENAME = ".code-reality.toml"
DEFAULT_EXCLUDE = (".venv/",)
_CLAIM_SEG = r"[a-z_0-9]+"


@dataclass(frozen=True)
class ModuleRule:
    prefix: str
    depth: int = 1


@dataclass(frozen=True)
class ScanRoot:
    path: str  # rust 掃描 glob（repo 相對）
    pyi: str  # .pyi 合約樹 glob（repo 相對）


@dataclass(frozen=True)
class HazardRegistry:
    """registry auto-discovery 事實（hazard 規則用）——repo 事實歸 repo。"""

    package_prefix: str  # registry 掃描的 package 路徑前綴（目錄粒度）
    suffix: str  # class 名慣例後綴（如 "Condition"）
    register_fn: str  # auto-discovery 註冊函數名
    registry: str  # registry 容器名
    evidence: str = ""  # 註冊鏈證據（path:line，顯示用）


@dataclass(frozen=True)
class Profile:
    modules: tuple[ModuleRule, ...] = ()
    exclude: tuple[str, ...] = DEFAULT_EXCLUDE
    scan_roots: tuple[ScanRoot, ...] = ()
    hazard_registries: tuple[HazardRegistry, ...] = ()


def load_profile(repo_root: Path) -> Profile | None:
    """載入 repo root 的 profile；無檔回 None（caller 走 generic fallback）。

    壞 TOML／schema 不合 → crash-only（fail-fast，不猜測意圖）。
    前綴一律目錄粒度（帶斜線）——無斜線條目會誤傷同名開頭檔
    （``.venv-setup.py``，與 exclusions 同理）。
    """
    path = repo_root / PROFILE_FILENAME
    if not path.exists():
        return None
    try:
        data = tomllib.loads(path.read_text())
    except tomllib.TOMLDecodeError as e:
        raise AssertionError(f"{path} TOML 解析失敗：{e}") from e
    unknown = set(data) - {"module", "exclude", "scan_root", "hazard_registry"}
    assert not unknown, (
        f"{path} 含未知鍵 {sorted(unknown)}——拼錯 section 名會靜默退化 generic "
        "fallback（合法鍵：module／exclude／scan_root）"
    )
    try:
        modules = tuple(
            ModuleRule(prefix=m["prefix"], depth=m.get("depth", 1))
            for m in data.get("module", [])
        )
        exclude = tuple(data.get("exclude", DEFAULT_EXCLUDE))
        roots = tuple(
            ScanRoot(path=s["path"], pyi=s["pyi"]) for s in data.get("scan_root", [])
        )
        registries = tuple(
            HazardRegistry(
                package_prefix=r["package_prefix"],
                suffix=r["suffix"],
                register_fn=r["register_fn"],
                registry=r["registry"],
                evidence=r.get("evidence", ""),
            )
            for r in data.get("hazard_registry", [])
        )
    except KeyError as e:
        raise AssertionError(
            f"{path} schema 不合（缺 {e}）——[[module]] 需 prefix（depth 可選）、"
            "[[scan_root]] 需 path＋pyi、[[hazard_registry]] 需 package_prefix＋"
            "suffix＋register_fn＋registry（evidence 可選）"
        ) from e
    for rule in modules:
        assert rule.prefix.endswith("/"), (
            f"{path} [[module]] prefix={rule.prefix!r} 須以 / 結尾（目錄粒度）"
        )
        assert (
            isinstance(rule.depth, int)
            and not isinstance(rule.depth, bool)
            and rule.depth >= 1
        ), f"{path} [[module]] depth={rule.depth!r} 須為 >= 1 的整數"
    for prefix in exclude:
        assert prefix.endswith("/"), (
            f"{path} exclude={prefix!r} 須以 / 結尾（目錄粒度）"
        )
    for reg in registries:
        assert reg.package_prefix.endswith("/"), (
            f"{path} [[hazard_registry]] package_prefix={reg.package_prefix!r} "
            "須以 / 結尾（目錄粒度）"
        )
    return Profile(
        modules=modules, exclude=exclude, scan_roots=roots, hazard_registries=registries
    )


def module_of(rel_path: str, profile: Profile | None) -> str:
    """repo 相對路徑 → module 名。

    有規則：有序首中；prefix 下第 ``depth`` 層目錄；prefix 根檔案
    （路徑段含副檔名）歸 prefix 本身（F6）。無規則／無 profile：
    頂層目錄（根檔案即檔名——現行 generic 行為）。
    """
    if profile is not None:
        for rule in profile.modules:
            if rel_path.startswith(rule.prefix):
                base = rule.prefix.rstrip("/")
                rest = rel_path[len(rule.prefix) :]
                if not rest:
                    return base
                segments = rest.split("/")[: rule.depth]
                if any("." in seg for seg in segments):
                    return base
                return f"{base}/{'/'.join(segments)}"
    return rel_path.split("/")[0]


def claims_re(profile: Profile | None) -> re.Pattern[str]:
    """[[module]] prefixes 衍生 EP claims 抓取 regex（V3 POC 等價驗證）。

    無規則 → 永不命中（claims 是 profile 能力；generic repo 無前綴知識，
    delta_tour 的宣稱標註不生效——by design）。已知邊界：多 prefix **重疊**
    時（如 ``crates/``＋``crates/live/``），regex 位置式匹配與 module_of
    的有序首中在「prefix 根檔案路徑出現在 EP 行文」場景粒度可能分歧——
    現行 profile 皆單規則不觸發；重疊配置自行驗證對照粒度。
    """
    if profile is None or not profile.modules:
        return re.compile(r"(?!x)x")
    alts = "|".join(re.escape(rule.prefix.rstrip("/")) for rule in profile.modules)
    return re.compile(rf"(?:{alts})/{_CLAIM_SEG}")


def scan_roots(profile: Profile | None) -> tuple[ScanRoot, ...]:
    """boundary 掃描根；無 profile／缺 [[scan_root]] → 空 tuple。

    caller（boundary_build）對空集 crash-only：顯式 ``--repo`` ＋該 repo
    profile 定義 [[scan_root]] 才能掃（SM-1b/G4——不內建任何 repo 預設）。
    """
    return profile.scan_roots if profile is not None else ()

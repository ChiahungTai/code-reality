"""profile 引擎測試——loader crash-only、module_of 規則表（含 F6 根檔案）、
claims 衍生等價（POC-3 提煉）、scan_roots fallback。

POC 對應：poc_profile_claims_derivation.py（claims 衍生對 mosaic 等價——
build 時提煉，行為以本測釘住）。
"""

import re
from pathlib import Path

import pytest
from profile_repo import NT_PROFILE, write_mosaic_profile

from code_reality.profile import (
    HazardRegistry,
    ModuleRule,
    Profile,
    claims_re,
    load_profile,
    module_of,
    scan_roots,
)

MOSAIC_MOD = ModuleRule(prefix="mosaic_alpha/")


class TestLoadProfile:
    def test_no_file_returns_none(self, tmp_path: Path) -> None:
        assert load_profile(tmp_path) is None

    def test_mosaic_shape(self, tmp_path: Path) -> None:
        write_mosaic_profile(tmp_path)
        profile = load_profile(tmp_path)
        assert profile is not None
        assert profile.modules == (MOSAIC_MOD,)
        assert profile.exclude == ("stubs/", "ai-analysis/", ".venv/", "snapshot/")
        assert profile.scan_roots == ()

    def test_nt_scan_roots(self, tmp_path: Path) -> None:
        (tmp_path / ".code-reality.toml").write_text(NT_PROFILE)
        profile = load_profile(tmp_path)
        assert profile is not None
        assert [sr.path for sr in profile.scan_roots] == ["crates/**/*.rs"]
        assert [sr.pyi for sr in profile.scan_roots] == [
            "python/nautilus_trader/**/*.pyi"
        ]

    def test_bad_toml_crash(self, tmp_path: Path) -> None:
        (tmp_path / ".code-reality.toml").write_text("exclude = [unclosed")
        with pytest.raises(AssertionError, match="TOML"):
            load_profile(tmp_path)

    def test_missing_key_crash(self, tmp_path: Path) -> None:
        (tmp_path / ".code-reality.toml").write_text(
            '[[module]]\nprefix = "x/"\n[[scan_root]]\n'
        )
        with pytest.raises(AssertionError, match="scan_root"):
            load_profile(tmp_path)

    def test_prefix_without_slash_crash(self, tmp_path: Path) -> None:
        (tmp_path / ".code-reality.toml").write_text(
            '[[module]]\nprefix = "mosaic_alpha"\n'
        )
        with pytest.raises(AssertionError, match="目錄粒度"):
            load_profile(tmp_path)

    def test_unknown_key_crash(self, tmp_path: Path) -> None:
        """review F1：拼錯 section 鍵（[[modules]]）不得靜默退化 generic。"""
        (tmp_path / ".code-reality.toml").write_text(
            '[[modules]]\nprefix = "mosaic_alpha/"\n'
        )
        with pytest.raises(AssertionError, match="未知鍵"):
            load_profile(tmp_path)

    def test_float_depth_crash(self, tmp_path: Path) -> None:
        """review F4：float depth 在載入點攔（非延遲到 module_of slice 爆）。"""
        (tmp_path / ".code-reality.toml").write_text(
            '[[module]]\nprefix = "mosaic_alpha/"\ndepth = 1.5\n'
        )
        with pytest.raises(AssertionError, match="整數"):
            load_profile(tmp_path)


class TestModuleOf:
    """規則表含 F6（prefix 根檔案歸 prefix 本身）＋有序首中＋generic fallback。"""

    def test_first_level_dir(self) -> None:
        profile = Profile(modules=(MOSAIC_MOD,))
        assert module_of("mosaic_alpha/common/enums.py", profile) == (
            "mosaic_alpha/common"
        )

    def test_root_file_goes_to_prefix_itself(self) -> None:
        # F6：prefix 根檔案（含副檔名）歸 prefix 本身——.py 與非 .py 同判
        profile = Profile(modules=(MOSAIC_MOD,))
        assert module_of("mosaic_alpha/__init__.py", profile) == "mosaic_alpha"
        assert module_of("mosaic_alpha/AGENTS.md", profile) == "mosaic_alpha"

    def test_unchanged_from_legacy_semantics_table(self) -> None:
        """mosaic 等價 fixture 表——與舊 hardcoded module_of 逐條一致（SM-2 前提）。"""
        profile = Profile(modules=(MOSAIC_MOD,))
        table = {
            "mosaic_alpha/common/enums.py": "mosaic_alpha/common",
            "mosaic_alpha/__init__.py": "mosaic_alpha",
            "mosaic_alpha/AGENTS.md": "mosaic_alpha",
            "tests/unit_tests/a.py": "tests",
            "tools/other/x.py": "tools",
            "README.md": "README.md",
        }
        for rel, expected in table.items():
            assert module_of(rel, profile) == expected, rel

    def test_depth_two_semantics(self) -> None:
        """depth=2（review F3）：module＝prefix 下第 2 層目錄；根檔案與
        淺於 depth 的路徑（第 2 層是檔案）都歸 base。"""
        profile = Profile(modules=(ModuleRule(prefix="crates/", depth=2),))
        assert module_of("crates/lib.rs", profile) == "crates"
        assert module_of("crates/live/x.rs", profile) == "crates"
        assert module_of("crates/live/src/x.rs", profile) == "crates/live/src"

    def test_no_profile_top_level_fallback(self) -> None:
        assert module_of("tests/unit_tests/a.py", None) == "tests"
        assert module_of("mosaic_alpha/data/x.py", None) == "mosaic_alpha"

    def test_ordered_first_match(self) -> None:
        profile = Profile(
            modules=(ModuleRule(prefix="crates/live/"), ModuleRule(prefix="crates/"))
        )
        assert module_of("crates/live/x.rs", profile) == "crates/live"
        assert module_of("crates/common/x.rs", profile) == "crates/common"


class TestClaimsRe:
    def test_derived_equivalent_to_legacy_mosaic_regex(self) -> None:
        """POC-3 等價：衍生 regex 對 mosaic 樣本 ⊇ 舊 regex 且命中集合相同。"""
        legacy = re.compile(r"mosaic_alpha/[a-z_0-9]+")
        derived = claims_re(Profile(modules=(MOSAIC_MOD,)))
        for text in (
            "宣稱觸及 `mosaic_alpha/services`（momentum）",
            "動 `mosaic_alpha/trading` 與 `mosaic_alpha/strategies`",
            "`mosaic_alpha/nonexistent` 沒做",
            "規則樹 alpha_forge（無路徑）",
        ):
            assert set(legacy.findall(text)) == set(derived.findall(text)), text

    def test_multi_prefix_alternation(self) -> None:
        profile = Profile(modules=(MOSAIC_MOD, ModuleRule(prefix="crates/")))
        got = claims_re(profile).findall("crates/common 與 mosaic_alpha/data")
        assert set(got) == {"crates/common", "mosaic_alpha/data"}

    def test_no_profile_never_matches(self) -> None:
        assert claims_re(None).findall("mosaic_alpha/data") == []
        assert claims_re(Profile()).findall("mosaic_alpha/data") == []


class TestScanRoots:
    def test_none_when_no_profile(self) -> None:
        assert scan_roots(None) == ()

    def test_none_when_profile_lacks_scan_root(self, tmp_path: Path) -> None:
        # G4：有 profile 缺 [[scan_root]] → 視同無（caller crash-only）
        write_mosaic_profile(tmp_path)
        profile = load_profile(tmp_path)
        assert scan_roots(profile) == ()


class TestHazardRegistry:
    """[[hazard_registry]]——registry auto-discovery 偵測的 repo 事實單一源。"""

    REGISTRY_TOML = """\
[[hazard_registry]]
package_prefix = "mosaic_alpha/conditions/"
suffix = "Condition"
register_fn = "auto_register_conditions"
registry = "CONDITION_REGISTRY"
evidence = "mosaic_alpha/conditions/discovery.py:149"
"""

    def test_parse_full_entry(self, tmp_path: Path) -> None:
        (tmp_path / ".code-reality.toml").write_text(self.REGISTRY_TOML)
        profile = load_profile(tmp_path)
        assert profile is not None
        assert profile.hazard_registries == (
            HazardRegistry(
                package_prefix="mosaic_alpha/conditions/",
                suffix="Condition",
                register_fn="auto_register_conditions",
                registry="CONDITION_REGISTRY",
                evidence="mosaic_alpha/conditions/discovery.py:149",
            ),
        )

    def test_evidence_optional(self, tmp_path: Path) -> None:
        (tmp_path / ".code-reality.toml").write_text(
            self.REGISTRY_TOML.split("evidence")[0]
        )
        profile = load_profile(tmp_path)
        assert profile is not None
        assert profile.hazard_registries[0].evidence == ""

    def test_prefix_without_slash_crash(self, tmp_path: Path) -> None:
        (tmp_path / ".code-reality.toml").write_text(
            self.REGISTRY_TOML.replace(
                'package_prefix = "mosaic_alpha/conditions/"',
                'package_prefix = "mosaic_alpha/conditions"',
            )
        )
        with pytest.raises(AssertionError, match="目錄粒度"):
            load_profile(tmp_path)

    def test_missing_required_key_crash(self, tmp_path: Path) -> None:
        (tmp_path / ".code-reality.toml").write_text(
            self.REGISTRY_TOML.replace('suffix = "Condition"\n', "")
        )
        with pytest.raises(AssertionError, match="hazard_registry"):
            load_profile(tmp_path)

    def test_default_empty_tuple(self, tmp_path: Path) -> None:
        write_mosaic_profile(tmp_path)
        profile = load_profile(tmp_path)
        assert profile is not None
        assert profile.hazard_registries == ()

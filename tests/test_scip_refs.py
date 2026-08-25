"""scip_refs 單元測試——duck-typed fake index（不依賴 protobuf/scip_pb2）。

釘住查詢匹配邏輯（impl 變體＋trait 宣告位址兩符號形態＋``(?<!\\w)``
邊界）、DEF/refs 收集、audit **雙鍵歸屬**（(定義檔, 方法名)——只按檔
會把同檔鄰居 refs 聯集，原型審查 216→138 實證）、main() 退出碼契約
（0/1/2）與 load_index 截斷 sanity；repo-keyed 預設 slot（--repo 時
--index 可省略）與 [SRC] source 標註（stamp sidecar＋live HEAD＋漂移
守衛；顯式 --index 無證據時輸出位元組不變——NT 契約）；衍生 sqlite
查詢面（--build-cache 三表、SqliteFace 與 protobuf 掃描等價、open_face
路由與過期雙訊號、**兩路徑 stdout 位元組相同**）。真索引 L4 基準：
NT 實測 --audit 138/861（.agent-tmp/research/scip/ 原型輪）。
"""

import json
import os
import sqlite3
import subprocess
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest

from code_reality import scip_refs
from code_reality.scip_refs import (
    _matcher,
    audit_targets,
    find_defs,
    find_refs,
    ln,
    load_index,
    loc,
    main,
    missing_refs,
    tail,
)


class FakeOcc:
    def __init__(self, symbol: str, roles: int, rng: list[int]):
        self.symbol = symbol
        self.symbol_roles = roles
        self.range = rng


class FakeDoc:
    def __init__(self, rel: str, occurrences: list[FakeOcc]):
        self.relative_path = rel
        self.occurrences = occurrences


class FakeIndex:
    def __init__(self, docs: list[FakeDoc]):
        self.documents = docs


IMPL = (
    "rust-analyzer cargo nautilus 1.0.0 crates/common/src/events.rs "
    "impl#[EventStoreLifecycle]open()."
)
TRAIT_IMPL = (
    "rust-analyzer cargo nautilus 1.0.0 crates/common/src/events.rs "
    "impl#[EventStoreLifecycle][EventStore]open()."
)
TRAIT_DECL = (
    "rust-analyzer cargo nautilus 1.0.0 crates/common/src/events.rs "
    "EventStoreLifecycle#open()."
)
OTHER_TYPE = (
    "rust-analyzer cargo nautilus 1.0.0 crates/common/src/other.rs "
    "impl#[OtherType]open()."
)


class TestMatcher:
    def test_type_method_matches_all_three_forms(self) -> None:
        match = _matcher("EventStoreLifecycle.open")
        assert match(IMPL)
        assert match(TRAIT_IMPL)
        assert match(TRAIT_DECL)  # trait 宣告位址——漏它＝低報 refs

    def test_type_method_rejects_other_type(self) -> None:
        match = _matcher("EventStoreLifecycle.open")
        assert not match(OTHER_TYPE)

    def test_word_boundary_rejects_prefixed_names(self) -> None:
        match = _matcher("EventStoreLifecycle.open")
        assert not match(IMPL.replace("]open().", "]my_open()."))
        assert not match(IMPL.replace("]open().", "]reopen()."))

    def test_bare_name_matches_any_type(self) -> None:
        match = _matcher("open_run")
        assert match(
            IMPL.replace("open().", "open_run().").replace(
                "[EventStoreLifecycle]", "[Config]"
            )
        )
        assert not match(IMPL.replace("open().", "my_open_run()."))

    def test_bare_name_ignores_type_marker(self) -> None:
        """裸查詢不含型別——任何 marker 上的同名方法都命中。"""
        match = _matcher("open")
        assert match(IMPL)
        assert match(OTHER_TYPE)


class TestFindDefsRefs:
    def test_defs_collect_def_role_only(self) -> None:
        idx = FakeIndex(
            [
                FakeDoc(
                    "crates/x.rs",
                    [
                        FakeOcc(IMPL, 1, [10, 0, 10, 5]),
                        FakeOcc(IMPL, 0, [30, 0, 30, 5]),  # ref——非 DEF
                        FakeOcc(OTHER_TYPE, 1, [50, 0, 50, 5]),
                    ],
                )
            ]
        )
        defs = find_defs(idx, "EventStoreLifecycle.open")
        assert list(defs) == [IMPL]
        assert defs[IMPL] == ["crates/x.rs:11"]

    def test_refs_collect_non_def_of_known_symbols(self) -> None:
        idx = FakeIndex(
            [
                FakeDoc(
                    "crates/y.rs",
                    [
                        FakeOcc(IMPL, 0, [7, 0, 7, 9]),
                        FakeOcc(TRAIT_DECL, 0, [9, 0, 9, 9]),
                        FakeOcc(OTHER_TYPE, 0, [11, 0, 11, 9]),  # 不在查詢集
                    ],
                )
            ]
        )
        refs = find_refs(idx, {IMPL, TRAIT_DECL})
        assert refs[IMPL] == ["crates/y.rs:8"]
        assert refs[TRAIT_DECL] == ["crates/y.rs:10"]
        assert OTHER_TYPE not in refs


class TestLocHelpers:
    def test_ln_one_based(self) -> None:
        assert ln(FakeOcc("s", 0, [4, 0, 4, 9])) == 5

    def test_ln_empty_range(self) -> None:
        assert ln(FakeOcc("s", 0, [])) == -1

    def test_loc_unknown_line(self) -> None:
        assert loc("f.rs", FakeOcc("s", 0, [])) == "f.rs:?"
        assert loc("f.rs", FakeOcc("s", 0, [2, 0, 2, 1])) == "f.rs:3"

    def test_tail_extracts_descriptor(self) -> None:
        assert tail(IMPL) == "impl#[EventStoreLifecycle]open()."
        assert tail("short.rs x#y().") == "short.rs x#y()."


class TestAuditDualKey:
    """(定義檔, 方法名) 雙鍵歸屬——同檔鄰居（不同方法名）refs 不得聯集。"""

    DOC = FakeDoc(
        "crates/x.rs",
        [
            FakeOcc("… impl#[A]push().", 1, [10, 0, 10, 5]),
            FakeOcc("… impl#[B]push().", 1, [20, 0, 20, 5]),
            FakeOcc("… impl#[A]pull().", 1, [30, 0, 30, 5]),  # 同檔鄰居方法
            FakeOcc("… impl#[A]push().", 0, [40, 0, 40, 5]),  # ref——非 DEF
        ],
    )

    def test_targets_match_missing_name_in_file(self) -> None:
        targets = audit_targets([self.DOC], {"push": {"crates/x.rs"}})
        # pull 不在缺差清單 → 不入 targets；push 的兩 impl 變體都入
        assert set(targets) == {"… impl#[A]push().", "… impl#[B]push()."}
        assert targets["… impl#[A]push()."] == ("crates/x.rs", "push")

    def test_targets_skip_files_not_in_missing(self) -> None:
        targets = audit_targets([self.DOC], {"push": {"crates/other.rs"}})
        assert targets == {}

    def test_missing_refs_unions_same_name_only(self) -> None:
        targets = audit_targets([self.DOC], {"push": {"crates/x.rs"}})
        refs_count = {
            "… impl#[A]push().": ["crates/y.rs:1"],
            "… impl#[B]push().": ["crates/z.rs:2"],
            "… impl#[A]pull().": ["crates/w.rs:3"],
        }
        m = {"symbol": "push", "_rel": "crates/x.rs"}
        # push 兩 impl 變體 refs 聯集（同 (檔, 名) 歸屬）；pull 不混入
        assert missing_refs(m, targets, refs_count) == [
            "crates/y.rs:1",
            "crates/z.rs:2",
        ]


class FakePb2Index:
    """load_index 測試替身——duck-typed scip_pb2.Index。"""

    def __init__(self, n_docs: int = 0, corrupt: bool = False):
        self._n = n_docs
        self._corrupt = corrupt
        self.documents = []

    def ParseFromString(self, data: bytes) -> None:
        if self._corrupt:
            raise ValueError("DecodeError: truncated")
        self.documents = [FakeDoc("f.rs", []) for _ in range(self._n)]


class TestLoadIndex:
    """截斷 sanity（教訓③第三項）——protobuf 無完整性校驗，截斷靜默假
    「查無」是這條 sanity 存在的全部理由。"""

    def _patch(self, monkeypatch, **kw) -> FakePb2Index:
        fake = FakePb2Index(**kw)
        monkeypatch.setattr(scip_refs, "scip_pb2", SimpleNamespace(Index=lambda: fake))
        return fake

    def test_corrupt_index_exits_2(self, tmp_path, monkeypatch, capsys) -> None:
        self._patch(monkeypatch, corrupt=True)
        p = tmp_path / "index.scip"
        p.write_bytes(b"junk")
        with pytest.raises(SystemExit) as ei:
            load_index(p)
        assert ei.value.code == 2
        assert "[FAIL]" in capsys.readouterr().err

    def test_zero_documents_exits_2(self, tmp_path, monkeypatch) -> None:
        self._patch(monkeypatch, n_docs=0)
        p = tmp_path / "index.scip"
        p.write_bytes(b"junk")
        with pytest.raises(SystemExit) as ei:
            load_index(p)
        assert ei.value.code == 2

    def test_small_index_warns_but_returns(self, tmp_path, monkeypatch, capsys) -> None:
        fake = self._patch(monkeypatch, n_docs=50)
        p = tmp_path / "index.scip"
        p.write_bytes(b"junk")
        idx = load_index(p)
        assert idx is fake
        assert len(idx.documents) == 50
        assert "[WARN]" in capsys.readouterr().err

    def test_healthy_index_no_warn(self, tmp_path, monkeypatch, capsys) -> None:
        self._patch(monkeypatch, n_docs=200)
        p = tmp_path / "index.scip"
        p.write_bytes(b"junk")
        load_index(p)
        assert "[WARN]" not in capsys.readouterr().err


class TestMainExitCodes:
    """退出碼契約 0/1/2（教訓③——NT 治理鉤子未來接線時漂移即靜默）。"""

    def _run(self, monkeypatch, argv: list[str]) -> int:
        monkeypatch.setattr(sys, "argv", ["scip_refs", *argv])
        return main()

    def _idx(self, tmp_path) -> Path:
        p = tmp_path / "index.scip"
        p.write_bytes(b"junk")
        return p

    def test_protobuf_missing_returns_2(self, tmp_path, monkeypatch) -> None:
        monkeypatch.setattr(scip_refs, "scip_pb2", None)
        assert self._run(monkeypatch, ["q", "--index", str(self._idx(tmp_path))]) == 2

    def test_audit_query_mutually_exclusive_returns_2(
        self, tmp_path, monkeypatch
    ) -> None:
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        assert (
            self._run(
                monkeypatch,
                [
                    "q",
                    "--audit",
                    "--repo",
                    str(tmp_path),
                    "--index",
                    str(self._idx(tmp_path)),
                ],
            )
            == 2
        )

    def test_audit_without_repo_returns_2(self, tmp_path, monkeypatch) -> None:
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        assert (
            self._run(monkeypatch, ["--audit", "--index", str(self._idx(tmp_path))])
            == 2
        )

    def test_missing_index_returns_2(self, tmp_path, monkeypatch) -> None:
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        missing = tmp_path / "nope.scip"
        assert self._run(monkeypatch, ["q", "--index", str(missing)]) == 2

    def test_no_query_returns_2(self, tmp_path, monkeypatch) -> None:
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        assert self._run(monkeypatch, ["--index", str(self._idx(tmp_path))]) == 2

    def test_no_query_skips_source_line(self, tmp_path, monkeypatch, capsys) -> None:
        """審查 F6：無 query/audit 的錯誤路徑不跑 source_line——不多印
        meta WARN、不觸 git。"""
        monkeypatch.setattr(scip_refs, "scip_pb2", object())

        def boom(repo):
            raise AssertionError("source_line 不應觸 git")

        monkeypatch.setattr(scip_refs, "_git_head", boom)
        assert self._run(monkeypatch, ["--index", str(self._idx(tmp_path))]) == 2
        err = capsys.readouterr().err
        assert "需提供查詢" in err
        assert "未 stamp" not in err

    def test_query_no_def_returns_1(self, tmp_path, monkeypatch) -> None:
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        monkeypatch.setattr(scip_refs, "load_index", lambda p: FakeIndex([]))
        assert (
            self._run(monkeypatch, ["whatever", "--index", str(self._idx(tmp_path))])
            == 1
        )

    def test_query_with_def_returns_0(self, tmp_path, monkeypatch) -> None:
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        idx = FakeIndex([FakeDoc("crates/x.rs", [FakeOcc(IMPL, 1, [1, 0, 1, 5])])])
        monkeypatch.setattr(scip_refs, "load_index", lambda p: idx)
        assert (
            self._run(
                monkeypatch,
                ["EventStoreLifecycle.open", "--index", str(self._idx(tmp_path))],
            )
            == 0
        )


class TestRepoKeyedIndex:
    """①repo-keyed slot——多 repo 互蓋防護；顯式 --index 永遠優先。"""

    def test_default_slot_resolves_repo_basename(self, tmp_path, monkeypatch) -> None:
        monkeypatch.setattr(scip_refs, "DEFAULT_INDEX_ROOT", tmp_path / "scip")
        got = scip_refs.default_index_path(Path("/Users/x/Github/nt_v1"))
        assert got == tmp_path / "scip" / "nt_v1" / "index.scip"

    def test_relative_repo_resolves_cwd_basename(self, tmp_path, monkeypatch) -> None:
        """F1 釘住——不 resolve 的 ``Path('.').name`` 是空字串，會塌縮回
        全局單檔（①要防的互蓋）。"""
        monkeypatch.setattr(scip_refs, "DEFAULT_INDEX_ROOT", tmp_path / "scip")
        repo_dir = tmp_path / "nt_v1"
        repo_dir.mkdir()
        monkeypatch.chdir(repo_dir)
        assert scip_refs.default_index_path(Path(".")) == (
            tmp_path / "scip" / "nt_v1" / "index.scip"
        )


class TestMainIndexResolution:
    """①main 接線——--index 省略時走 --repo 預設 slot。"""

    def _run(self, monkeypatch, argv: list[str]) -> int:
        monkeypatch.setattr(sys, "argv", ["scip_refs", *argv])
        return main()

    def test_no_index_no_repo_returns_2(self, tmp_path, monkeypatch) -> None:
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        assert self._run(monkeypatch, ["q"]) == 2

    def test_default_slot_missing_returns_2(
        self, tmp_path, monkeypatch, capsys
    ) -> None:
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        monkeypatch.setattr(scip_refs, "DEFAULT_INDEX_ROOT", tmp_path / "scip")
        assert self._run(monkeypatch, ["q", "--repo", str(tmp_path / "myrepo")]) == 2
        err = capsys.readouterr().err
        assert "預設索引不在" in err
        assert "myrepo/index.scip" in err

    def test_default_slot_missing_with_legacy_shows_migration_hint(
        self, tmp_path, monkeypatch, capsys
    ) -> None:
        """審查 F1：legacy 全局 slot 有索引時——錯誤訊息附搬遷命令（免 8
        分鐘重生成誘導）。"""
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        monkeypatch.setattr(scip_refs, "DEFAULT_INDEX_ROOT", tmp_path / "scip")
        (tmp_path / "scip").mkdir()
        (tmp_path / "scip" / "index.scip").write_bytes(b"junk")
        rc = self._run(monkeypatch, ["q", "--repo", str(tmp_path / "myrepo")])
        assert rc == 2
        err = capsys.readouterr().err
        assert "搬遷" in err and "mv" in err

    def test_audit_via_default_slot_resolves(self, tmp_path, monkeypatch) -> None:
        """審查 F4d：--audit --repo 經 main() 的 slot 解析接線。"""
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        monkeypatch.setattr(scip_refs, "DEFAULT_INDEX_ROOT", tmp_path / "scip")
        monkeypatch.setattr(scip_refs, "_git_head", lambda repo: None)
        idx_file = tmp_path / "scip" / "myrepo" / "index.scip"
        idx_file.parent.mkdir(parents=True)
        idx_file.write_bytes(b"junk")
        seen: list[Path] = []

        def fake_audit(index_path, repo, src_line=None):
            seen.append(index_path)
            return 0

        monkeypatch.setattr(scip_refs, "audit_mode", fake_audit)
        rc = self._run(monkeypatch, ["--audit", "--repo", str(tmp_path / "myrepo")])
        assert rc == 0
        assert seen == [idx_file]

    def test_default_slot_hit_loads_it(self, tmp_path, monkeypatch) -> None:
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        monkeypatch.setattr(scip_refs, "DEFAULT_INDEX_ROOT", tmp_path / "scip")
        monkeypatch.setattr(scip_refs, "_git_head", lambda repo: None)  # F6：不觸真 git
        idx_file = tmp_path / "scip" / "myrepo" / "index.scip"
        idx_file.parent.mkdir(parents=True)
        idx_file.write_bytes(b"junk")
        seen: list[Path] = []
        idx = FakeIndex([FakeDoc("crates/x.rs", [FakeOcc(IMPL, 1, [1, 0, 1, 5])])])
        monkeypatch.setattr(scip_refs, "load_index", lambda p: (seen.append(p), idx)[1])
        rc = self._run(
            monkeypatch,
            ["EventStoreLifecycle.open", "--repo", str(tmp_path / "myrepo")],
        )
        assert rc == 0
        assert seen == [idx_file]


class TestStampMeta:
    """⑤資料面——sidecar 落地（repo/head/stamped_at）；冪等覆寫。"""

    def _run(self, monkeypatch, argv: list[str]) -> int:
        monkeypatch.setattr(sys, "argv", ["scip_refs", *argv])
        return main()

    def test_stamps_sidecar_next_to_index(self, tmp_path, monkeypatch, capsys) -> None:
        monkeypatch.setattr(scip_refs, "scip_pb2", None)  # stamp 不需 protobuf
        monkeypatch.setattr(scip_refs, "_git_head", lambda repo: "abcdef1234567890")
        idx = tmp_path / "index.scip"
        idx.write_bytes(b"junk")
        rc = self._run(
            monkeypatch,
            ["--stamp-meta", "--repo", str(tmp_path), "--index", str(idx)],
        )
        assert rc == 0
        meta = json.loads(
            (tmp_path / "index.scip.meta.json").read_text(encoding="utf-8")
        )
        assert meta["head"] == "abcdef1234567890"
        assert meta["repo"] == str(Path(tmp_path).resolve())
        assert meta["stamped_at"]
        assert "[OK] meta stamped" in capsys.readouterr().out

    def test_stamp_via_default_slot(self, tmp_path, monkeypatch) -> None:
        monkeypatch.setattr(scip_refs, "DEFAULT_INDEX_ROOT", tmp_path / "scip")
        monkeypatch.setattr(scip_refs, "scip_pb2", None)
        monkeypatch.setattr(scip_refs, "_git_head", lambda repo: "abcdef1234")
        idx = tmp_path / "scip" / "myrepo" / "index.scip"
        idx.parent.mkdir(parents=True)
        idx.write_bytes(b"junk")
        rc = self._run(
            monkeypatch, ["--stamp-meta", "--repo", str(tmp_path / "myrepo")]
        )
        assert rc == 0
        assert (idx.parent / "index.scip.meta.json").exists()

    def test_second_stamp_latest_wins(self, tmp_path, monkeypatch) -> None:
        """「重跑覆寫，冪等」docstring 宣稱的釘住（審查 F4b）。"""
        monkeypatch.setattr(scip_refs, "scip_pb2", None)
        heads = iter(["1111111111", "2222222222"])
        monkeypatch.setattr(scip_refs, "_git_head", lambda repo: next(heads))
        idx = tmp_path / "index.scip"
        idx.write_bytes(b"junk")
        argv = ["--stamp-meta", "--repo", str(tmp_path), "--index", str(idx)]
        assert self._run(monkeypatch, argv) == 0
        assert self._run(monkeypatch, argv) == 0
        meta = json.loads(
            (tmp_path / "index.scip.meta.json").read_text(encoding="utf-8")
        )
        assert meta["head"] == "2222222222"

    def test_stamp_without_repo_returns_2(self, tmp_path, monkeypatch) -> None:
        idx = tmp_path / "index.scip"
        idx.write_bytes(b"junk")
        assert self._run(monkeypatch, ["--stamp-meta", "--index", str(idx)]) == 2

    def test_stamp_git_fail_returns_2(self, tmp_path, monkeypatch, capsys) -> None:
        monkeypatch.setattr(scip_refs, "_git_head", lambda repo: None)
        idx = tmp_path / "index.scip"
        idx.write_bytes(b"junk")
        rc = self._run(
            monkeypatch,
            ["--stamp-meta", "--repo", str(tmp_path), "--index", str(idx)],
        )
        assert rc == 2
        assert "[FAIL]" in capsys.readouterr().err

    def test_stamp_mutually_exclusive_with_query(self, tmp_path, monkeypatch) -> None:
        idx = tmp_path / "index.scip"
        idx.write_bytes(b"junk")
        rc = self._run(
            monkeypatch,
            ["q", "--stamp-meta", "--repo", str(tmp_path), "--index", str(idx)],
        )
        assert rc == 2

    def test_stamp_mutually_exclusive_with_audit(self, tmp_path, monkeypatch) -> None:
        idx = tmp_path / "index.scip"
        idx.write_bytes(b"junk")
        rc = self._run(
            monkeypatch,
            ["--stamp-meta", "--audit", "--repo", str(tmp_path), "--index", str(idx)],
        )
        assert rc == 2


class TestSourceLine:
    """⑤輸出面——[SRC] 標註；無證據不捏造（legacy 輸出不變）。"""

    def _sidecar(self, tmp_path, head: str) -> Path:
        idx = tmp_path / "index.scip"
        idx.write_bytes(b"junk")
        (tmp_path / "index.scip.meta.json").write_text(
            json.dumps(
                {"repo": "r", "head": head, "stamped_at": "2026-08-24T10:00:00+00:00"}
            ),
            encoding="utf-8",
        )
        return idx

    def test_sidecar_only(self, tmp_path, monkeypatch, capsys) -> None:
        idx = self._sidecar(tmp_path, "abcdef1234567890")
        got = scip_refs.source_line(idx, None)
        assert got == "[SRC] scip index @ abcdef1（2026-08-24）"
        assert capsys.readouterr().err == ""

    def test_repo_only_warns_unstamped(self, tmp_path, monkeypatch, capsys) -> None:
        idx = tmp_path / "index.scip"
        idx.write_bytes(b"junk")
        monkeypatch.setattr(scip_refs, "_git_head", lambda repo: "1122334455")
        assert scip_refs.source_line(idx, tmp_path) == "[SRC] repo HEAD @ 1122334"
        assert "未 stamp" in capsys.readouterr().err

    def test_no_evidence_returns_none(self, tmp_path, monkeypatch, capsys) -> None:
        idx = tmp_path / "index.scip"
        idx.write_bytes(b"junk")
        assert scip_refs.source_line(idx, None) is None
        assert capsys.readouterr().err == ""

    def test_head_mismatch_warns_drift(self, tmp_path, monkeypatch, capsys) -> None:
        idx = self._sidecar(tmp_path, "abcdef1234")
        monkeypatch.setattr(scip_refs, "_git_head", lambda repo: "9998887777")
        line = scip_refs.source_line(idx, tmp_path)
        assert "scip index @ abcdef1" in line
        assert "repo HEAD @ 9998887" in line
        assert "已離開 index 生成點" in capsys.readouterr().err

    def test_corrupt_sidecar_falls_back_to_repo(
        self, tmp_path, monkeypatch, capsys
    ) -> None:
        idx = tmp_path / "index.scip"
        idx.write_bytes(b"junk")
        (tmp_path / "index.scip.meta.json").write_text("{broken", encoding="utf-8")
        monkeypatch.setattr(scip_refs, "_git_head", lambda repo: "1122334455")
        assert scip_refs.source_line(idx, tmp_path) == "[SRC] repo HEAD @ 1122334"
        assert "損壞" in capsys.readouterr().err

    def test_non_dict_meta_warns_and_missing(self, tmp_path, capsys) -> None:
        idx = tmp_path / "index.scip"
        idx.write_bytes(b"junk")
        (tmp_path / "index.scip.meta.json").write_text("[]", encoding="utf-8")
        assert scip_refs.source_line(idx, None) is None
        assert "形狀非預期" in capsys.readouterr().err

    def test_non_str_head_treated_missing(self, tmp_path, capsys) -> None:
        idx = tmp_path / "index.scip"
        idx.write_bytes(b"junk")
        (tmp_path / "index.scip.meta.json").write_text(
            json.dumps({"head": 123}), encoding="utf-8"
        )
        assert scip_refs.source_line(idx, None) is None
        assert "形狀非預期" in capsys.readouterr().err

    def test_git_head_non_repo_returns_none(self, tmp_path, capsys) -> None:
        """F6：真 argv 跑一次（非 repo 目錄）——非 None 路徑的實執行釘住。"""
        assert scip_refs._git_head(tmp_path) is None
        assert "git rev-parse 失敗" in capsys.readouterr().err

    def test_match_case_no_warn(self, tmp_path, monkeypatch, capsys) -> None:
        """（sidecar✓, repo✓）sha 一致＋repo 一致——零 WARN（審查 F4c）。"""
        idx = tmp_path / "index.scip"
        idx.write_bytes(b"junk")
        (tmp_path / "index.scip.meta.json").write_text(
            json.dumps(
                {
                    "repo": str(tmp_path.resolve()),
                    "head": "abcdef1234",
                    "stamped_at": "2026-08-24T10:00:00+00:00",
                }
            ),
            encoding="utf-8",
        )
        monkeypatch.setattr(scip_refs, "_git_head", lambda repo: "abcdef1234")
        assert scip_refs.source_line(idx, tmp_path) == (
            "[SRC] scip index @ abcdef1（2026-08-24） · repo HEAD @ abcdef1"
        )
        assert capsys.readouterr().err == ""

    def test_stale_stamp_mtime_warns(self, tmp_path, monkeypatch, capsys) -> None:
        """索引重生成後未重 stamp——sidecar mtime 較舊可機械偵測（審查 F2）。"""
        idx = tmp_path / "index.scip"
        idx.write_bytes(b"junk")
        sidecar = tmp_path / "index.scip.meta.json"
        sidecar.write_text(
            json.dumps({"repo": "r", "head": "abcdef1234"}), encoding="utf-8"
        )
        older = idx.stat().st_mtime - 100
        os.utime(sidecar, (older, older))
        monkeypatch.setattr(scip_refs, "_git_head", lambda repo: None)
        assert scip_refs.source_line(idx, None) is not None
        assert "比索引檔舊" in capsys.readouterr().err

    def test_repo_mismatch_warns(self, tmp_path, monkeypatch, capsys) -> None:
        """stamp 的 repo 與 --repo 不符（同名 basename）——sha 歸屬守衛
        （審查 F3）；sha 刻意相同以隔離 mismatch 軸（不觸漂移 WARN）。"""
        idx = tmp_path / "index.scip"
        idx.write_bytes(b"junk")
        (tmp_path / "index.scip.meta.json").write_text(
            json.dumps({"repo": "/other/place", "head": "abcdef1234"}),
            encoding="utf-8",
        )
        monkeypatch.setattr(scip_refs, "_git_head", lambda repo: "abcdef1234")
        assert scip_refs.source_line(idx, tmp_path) is not None
        err = capsys.readouterr().err
        assert "與 --repo 不符" in err
        assert "已離開" not in err


class TestGitHeadBranches:
    """_git_head 失敗三分支（審查 F4a）——stamp 與 [SRC] 的 git 邊界。"""

    def test_timeout_returns_none(self, monkeypatch, capsys) -> None:
        def fake_run(cmd, **kw):
            raise subprocess.TimeoutExpired(cmd, timeout=1)

        monkeypatch.setattr(scip_refs.subprocess, "run", fake_run)
        assert scip_refs._git_head(Path("/anywhere")) is None
        assert "逾時" in capsys.readouterr().err

    def test_git_missing_returns_none(self, monkeypatch, capsys) -> None:
        def fake_run(cmd, **kw):
            raise FileNotFoundError("git")

        monkeypatch.setattr(scip_refs.subprocess, "run", fake_run)
        assert scip_refs._git_head(Path("/anywhere")) is None
        assert "不在 PATH" in capsys.readouterr().err

    def test_nonzero_exit_returns_none(self, monkeypatch, capsys) -> None:
        proc = SimpleNamespace(returncode=128, stdout="", stderr="fatal: not a git")
        monkeypatch.setattr(scip_refs.subprocess, "run", lambda *a, **kw: proc)
        assert scip_refs._git_head(Path("/anywhere")) is None
        assert "git rev-parse 失敗" in capsys.readouterr().err


class TestReportSourceLine:
    """⑤接線——report/audit 首行 [SRC]；無證據時首行不變。"""

    def _run(self, monkeypatch, argv: list[str]) -> int:
        monkeypatch.setattr(sys, "argv", ["scip_refs", *argv])
        return main()

    def test_src_printed_first(self, capsys) -> None:
        idx = FakeIndex([FakeDoc("crates/x.rs", [FakeOcc(IMPL, 1, [1, 0, 1, 5])])])
        assert (
            scip_refs.report(
                scip_refs.ProtobufFace(idx), "EventStoreLifecycle.open", "[SRC] x @ y"
            )
            == 0
        )
        out = capsys.readouterr().out.splitlines()
        assert out[0] == "[SRC] x @ y"
        assert out[1].startswith("[OK]")

    def test_no_src_keeps_legacy_first_line(self, capsys) -> None:
        idx = FakeIndex([FakeDoc("crates/x.rs", [FakeOcc(IMPL, 1, [1, 0, 1, 5])])])
        assert (
            scip_refs.report(scip_refs.ProtobufFace(idx), "EventStoreLifecycle.open")
            == 0
        )
        out = capsys.readouterr().out.splitlines()
        assert out[0].startswith("[OK]")  # legacy 位元組不變——NT 契約釘住

    def test_main_query_sidecar_emits_src_first(
        self, tmp_path, monkeypatch, capsys
    ) -> None:
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        idx = tmp_path / "index.scip"
        idx.write_bytes(b"junk")
        (tmp_path / "index.scip.meta.json").write_text(
            json.dumps(
                {"head": "abcdef1234567890", "stamped_at": "2026-08-24T10:00:00+00:00"}
            ),
            encoding="utf-8",
        )
        fake = FakeIndex([FakeDoc("crates/x.rs", [FakeOcc(IMPL, 1, [1, 0, 1, 5])])])
        monkeypatch.setattr(scip_refs, "load_index", lambda p: fake)
        rc = self._run(monkeypatch, ["EventStoreLifecycle.open", "--index", str(idx)])
        assert rc == 0
        out = capsys.readouterr().out.splitlines()
        assert out[0] == "[SRC] scip index @ abcdef1（2026-08-24）"

    def test_main_query_no_evidence_no_src(self, tmp_path, monkeypatch, capsys) -> None:
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        idx = tmp_path / "index.scip"
        idx.write_bytes(b"junk")
        fake = FakeIndex([FakeDoc("crates/x.rs", [FakeOcc(IMPL, 1, [1, 0, 1, 5])])])
        monkeypatch.setattr(scip_refs, "load_index", lambda p: fake)
        rc = self._run(monkeypatch, ["EventStoreLifecycle.open", "--index", str(idx)])
        assert rc == 0
        out = capsys.readouterr().out
        assert "[SRC]" not in out

    def test_audit_prints_src_before_header(
        self, tmp_path, monkeypatch, capsys
    ) -> None:
        idx_file = tmp_path / "index.scip"
        idx_file.write_bytes(b"junk")
        monkeypatch.setattr(scip_refs, "load_index", lambda p: FakeIndex([]))
        proc = SimpleNamespace(returncode=0, stdout='{"missing": []}', stderr="")
        monkeypatch.setattr(scip_refs.subprocess, "run", lambda *a, **kw: proc)
        assert scip_refs.audit_mode(idx_file, tmp_path, "[SRC] audit @ z") == 0
        out = capsys.readouterr().out.splitlines()
        assert out[0] == "[SRC] audit @ z"
        assert out[1].startswith("[OK] graph_audit 缺差")


MY_OPEN = "… impl#[X]my_open()."
MY_OPEN_DASH = "… impl#[T]my-open()."  # FN_TAIL_RE 捕獲 open——method=? 縮小的超集邊界
REF_ONLY = (
    "rust-analyzer cargo nautilus 1.0.0 crates/common/src/dep.rs impl#[RefOnly]run()."
)
NON_FN = "rust-analyzer cargo nautilus 1.0.0 crates/common/src/types.rs SomeStruct"


def rich_index() -> FakeIndex:
    """等價/byte-identity 共用底稿——覆蓋三符號形態、邊界拒絕、ref-only、
    非函數形態、空 range、>6 refs（...共 N 處 行）、跨檔排序、dash 邊界。"""
    return FakeIndex(
        [
            FakeDoc(
                "crates/a.rs",
                [
                    FakeOcc(IMPL, 1, [10, 0, 10, 5]),
                    FakeOcc(OTHER_TYPE, 1, [1, 0, 1, 5]),
                    FakeOcc(MY_OPEN, 1, [2, 0, 2, 5]),
                    FakeOcc(MY_OPEN_DASH, 1, [3, 0, 3, 5]),
                    FakeOcc(NON_FN, 0, [4, 0, 4, 2]),
                ],
            ),
            FakeDoc(
                "crates/b.rs",
                [
                    FakeOcc(IMPL, 0, [7, 0, 7, 9]),
                    FakeOcc(IMPL, 0, [8, 0, 8, 9]),
                    FakeOcc(IMPL, 0, [9, 0, 9, 9]),
                    FakeOcc(IMPL, 0, [11, 0, 11, 9]),
                    FakeOcc(IMPL, 0, [12, 0, 12, 9]),
                    FakeOcc(IMPL, 0, [13, 0, 13, 9]),
                    FakeOcc(IMPL, 0, [14, 0, 14, 9]),
                    FakeOcc(IMPL, 0, [15, 0, 15, 9]),
                    FakeOcc(IMPL, 0, []),  # 空 range → "?"
                    FakeOcc(TRAIT_IMPL, 1, [5, 0, 5, 5]),
                    FakeOcc(TRAIT_DECL, 1, [6, 0, 6, 5]),
                    FakeOcc(TRAIT_DECL, 0, [9, 0, 9, 9]),
                    FakeOcc(REF_ONLY, 0, [3, 0, 3, 3]),
                ],
            ),
        ]
    )


def write_index_file(tmp_path: Path) -> Path:
    p = tmp_path / "index.scip"
    p.write_bytes(b"junk")
    return p


class TestBuildCache:
    """②資料面——--build-cache 落 <index>.scip.db 三表；occurrences 只收
    FN_TAIL_RE 符號（查詢消費集）；meta 記構建時 sidecar head。"""

    def _run(self, monkeypatch, argv: list[str]) -> int:
        monkeypatch.setattr(sys, "argv", ["scip_refs", *argv])
        return main()

    def test_build_via_cli_creates_db(self, tmp_path, monkeypatch, capsys) -> None:
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        idx_file = write_index_file(tmp_path)
        monkeypatch.setattr(scip_refs, "load_index", lambda p: rich_index())
        assert self._run(monkeypatch, ["--build-cache", "--index", str(idx_file)]) == 0
        db = tmp_path / "index.scip.db"
        assert db.exists()
        assert "cache built" in capsys.readouterr().out
        conn = sqlite3.connect(db)
        try:
            tails = dict(conn.execute("SELECT symbol, method FROM symbol_tails"))
            assert set(tails) == {
                IMPL,
                TRAIT_IMPL,
                TRAIT_DECL,
                OTHER_TYPE,
                MY_OPEN,
                MY_OPEN_DASH,
                REF_ONLY,
            }
            assert tails[IMPL] == "open"
            assert tails[MY_OPEN] == "my_open"
            assert tails[MY_OPEN_DASH] == "open"  # \w+ 捕獲停在 '-' 前
            # 非函數形態不入 occurrences；IMPL＝1 DEF＋9 refs（含空 range）
            assert (
                conn.execute(
                    "SELECT COUNT(*) FROM occurrences WHERE symbol = ?", (NON_FN,)
                ).fetchone()[0]
                == 0
            )
            assert (
                conn.execute(
                    "SELECT COUNT(*) FROM occurrences WHERE symbol = ?", (IMPL,)
                ).fetchone()[0]
                == 10
            )
            assert (
                conn.execute("SELECT value FROM meta WHERE key = 'head'").fetchone()[0]
                == ""
            )
        finally:
            conn.close()

    def test_build_stores_sidecar_head(self, tmp_path, monkeypatch) -> None:
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        idx_file = write_index_file(tmp_path)
        (tmp_path / "index.scip.meta.json").write_text(
            json.dumps({"repo": "r", "head": "abcdef1234"}), encoding="utf-8"
        )
        monkeypatch.setattr(scip_refs, "load_index", lambda p: rich_index())
        assert self._run(monkeypatch, ["--build-cache", "--index", str(idx_file)]) == 0
        conn = sqlite3.connect(tmp_path / "index.scip.db")
        try:
            assert (
                conn.execute("SELECT value FROM meta WHERE key = 'head'").fetchone()[0]
                == "abcdef1234"
            )
        finally:
            conn.close()

    def test_build_mutex_with_query(self, tmp_path, monkeypatch) -> None:
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        idx_file = write_index_file(tmp_path)
        assert (
            self._run(monkeypatch, ["q", "--build-cache", "--index", str(idx_file)])
            == 2
        )

    def test_build_mutex_with_audit_and_stamp(self, tmp_path, monkeypatch) -> None:
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        idx_file = write_index_file(tmp_path)
        rc1 = self._run(
            monkeypatch,
            [
                "--build-cache",
                "--audit",
                "--repo",
                str(tmp_path),
                "--index",
                str(idx_file),
            ],
        )
        rc2 = self._run(
            monkeypatch,
            [
                "--build-cache",
                "--stamp-meta",
                "--repo",
                str(tmp_path),
                "--index",
                str(idx_file),
            ],
        )
        assert rc1 == 2
        assert rc2 == 2

    def test_build_via_default_slot(self, tmp_path, monkeypatch) -> None:
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        monkeypatch.setattr(scip_refs, "DEFAULT_INDEX_ROOT", tmp_path / "scip")
        idx_file = tmp_path / "scip" / "myrepo" / "index.scip"
        idx_file.parent.mkdir(parents=True)
        idx_file.write_bytes(b"junk")
        monkeypatch.setattr(scip_refs, "load_index", lambda p: FakeIndex([]))
        assert (
            self._run(
                monkeypatch, ["--build-cache", "--repo", str(tmp_path / "myrepo")]
            )
            == 0
        )
        assert (tmp_path / "scip" / "myrepo" / "index.scip.db").exists()

    def test_build_sqlite_error_returns_2(self, tmp_path, monkeypatch, capsys) -> None:
        """審查 F1：sqlite3.Error 非 OSError 子類——CLI 失敗路須 exit 2 不裸
        traceback（docstring 明列「衍生 db 構建失敗」＝2）。"""
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        idx_file = write_index_file(tmp_path)
        monkeypatch.setattr(scip_refs, "load_index", lambda p: rich_index())

        def boom(index, db_path, head):
            raise sqlite3.OperationalError("database is locked")

        monkeypatch.setattr(scip_refs, "_build_db", boom)
        assert self._run(monkeypatch, ["--build-cache", "--index", str(idx_file)]) == 2
        assert "衍生 db 構建失敗" in capsys.readouterr().err

    def test_empty_query_with_build_cache_returns_2(
        self, tmp_path, monkeypatch
    ) -> None:
        """審查 F7：空字串查詢 falsy——互斥判斷用 is not None，不得靜默吞
        掉查詢意圖。"""
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        idx_file = write_index_file(tmp_path)
        monkeypatch.setattr(scip_refs, "load_index", lambda p: rich_index())
        assert (
            self._run(monkeypatch, ["", "--build-cache", "--index", str(idx_file)]) == 2
        )


class TestSqliteFaceEquivalence:
    """②等價層——SqliteFace 與 protobuf 掃描同底稿同輸出（byte-identical
    的單元釘住；SQL 候選縮小不得改變任何語義）。"""

    def _faces(self, tmp_path) -> tuple:
        idx = rich_index()
        idx_file = write_index_file(tmp_path)
        scip_refs._build_db(idx, scip_refs.sqlite_path(idx_file), "")
        db = scip_refs._open_ro(scip_refs.sqlite_path(idx_file))
        return scip_refs.ProtobufFace(idx), scip_refs.SqliteFace(db)

    def test_defs_type_method_equal(self, tmp_path) -> None:
        pb, sq = self._faces(tmp_path)
        assert sq.defs("EventStoreLifecycle.open") == pb.defs(
            "EventStoreLifecycle.open"
        )

    def test_defs_bare_name_equal(self, tmp_path) -> None:
        pb, sq = self._faces(tmp_path)
        assert sq.defs("open") == pb.defs("open")

    def test_defs_word_boundary_candidate_excluded(self, tmp_path) -> None:
        r"""my_open 在 tails 的 method 是 my_open——open 查詢的候選集就不含
        它（SQL 縮小與 (?<!\w) 邊界同律）。"""
        pb, sq = self._faces(tmp_path)
        assert sq.defs("X.my_open") == pb.defs("X.my_open")
        assert MY_OPEN not in sq.defs("open")

    def test_defs_non_word_method_query_equal(self, tmp_path) -> None:
        r"""審查 F2：query method 含非 \w 字元時 method=? 鍵對不上 FN_TAIL_RE
        捕獲（my-open 的 method 欄是 open）——縮小必須退全候選保超集。"""
        pb, sq = self._faces(tmp_path)
        got_pb, got_sq = pb.defs("T.my-open"), sq.defs("T.my-open")
        assert got_pb == got_sq
        assert list(got_sq) == [MY_OPEN_DASH]

    def test_defs_ref_only_symbol_absent(self, tmp_path) -> None:
        pb, sq = self._faces(tmp_path)
        assert sq.defs("RefOnly.run") == pb.defs("RefOnly.run") == {}

    def test_refs_equal_including_empty_and_cross_file(self, tmp_path) -> None:
        pb, sq = self._faces(tmp_path)
        symbols = {IMPL, TRAIT_DECL, OTHER_TYPE, REF_ONLY}
        assert sq.refs(symbols) == pb.refs(symbols)
        # IMPL refs 含空 range 的 "?" 行——兩路徑同型
        assert "crates/b.rs:?" in sq.refs(symbols)[IMPL]

    def test_audit_targets_equal(self, tmp_path) -> None:
        pb, sq = self._faces(tmp_path)
        files_by_name = {"open": {"crates/a.rs"}, "run": {"crates/b.rs"}}
        assert sq.audit_targets(files_by_name) == pb.audit_targets(files_by_name)


class TestOpenFaceRouting:
    """②路由——fresh db 走 sqlite（不觸 protobuf 解析）；無 db 走 protobuf
    原路徑；過期雙訊號（mtime＋sidecar head）WARN＋自動重建；重建失敗回
    protobuf 不擋服務。"""

    def test_no_db_uses_protobuf(self, tmp_path, monkeypatch) -> None:
        idx_file = write_index_file(tmp_path)
        seen: list[Path] = []
        monkeypatch.setattr(
            scip_refs, "load_index", lambda p: (seen.append(p), FakeIndex([]))[1]
        )
        assert isinstance(scip_refs.open_face(idx_file), scip_refs.ProtobufFace)
        assert seen == [idx_file]

    def test_fresh_db_skips_protobuf_parse(self, tmp_path, monkeypatch) -> None:
        idx_file = write_index_file(tmp_path)
        scip_refs._build_db(FakeIndex([]), scip_refs.sqlite_path(idx_file), "")

        def boom(p):
            raise AssertionError("fresh db 不應觸 protobuf 全量解析")

        monkeypatch.setattr(scip_refs, "load_index", boom)
        assert isinstance(scip_refs.open_face(idx_file), scip_refs.SqliteFace)

    def test_stale_mtime_warns_and_rebuilds(
        self, tmp_path, monkeypatch, capsys
    ) -> None:
        idx_file = write_index_file(tmp_path)
        db = scip_refs.sqlite_path(idx_file)
        scip_refs._build_db(FakeIndex([]), db, "")
        older = idx_file.stat().st_mtime - 100
        os.utime(db, (older, older))
        monkeypatch.setattr(scip_refs, "load_index", lambda p: FakeIndex([]))
        assert isinstance(scip_refs.open_face(idx_file), scip_refs.SqliteFace)
        err = capsys.readouterr().err
        assert "比索引檔舊" in err and "自動重建" in err
        # 重建已翻新 mtime——再開一次不再 WARN
        assert isinstance(scip_refs.open_face(idx_file), scip_refs.SqliteFace)
        assert "自動重建" not in capsys.readouterr().err

    def test_sidecar_head_change_triggers_rebuild(
        self, tmp_path, monkeypatch, capsys
    ) -> None:
        idx_file = write_index_file(tmp_path)
        scip_refs._build_db(FakeIndex([]), scip_refs.sqlite_path(idx_file), "")
        (tmp_path / "index.scip.meta.json").write_text(
            json.dumps({"repo": "r", "head": "abcdef1234"}), encoding="utf-8"
        )
        monkeypatch.setattr(scip_refs, "load_index", lambda p: FakeIndex([]))
        assert isinstance(scip_refs.open_face(idx_file), scip_refs.SqliteFace)
        assert "sidecar head 變動" in capsys.readouterr().err
        # 重建吃進新 head——第二次 fresh
        assert isinstance(scip_refs.open_face(idx_file), scip_refs.SqliteFace)
        assert "自動重建" not in capsys.readouterr().err

    def test_corrupt_db_rebuilt(self, tmp_path, monkeypatch, capsys) -> None:
        idx_file = write_index_file(tmp_path)
        db = scip_refs.sqlite_path(idx_file)
        db.write_bytes(b"definitely not sqlite")
        monkeypatch.setattr(scip_refs, "load_index", lambda p: rich_index())
        assert isinstance(scip_refs.open_face(idx_file), scip_refs.SqliteFace)
        assert "db 損壞" in capsys.readouterr().err
        conn = sqlite3.connect(db)
        try:
            # rich_index 的 FN 符號 occurrences 全量（IMPL 10＋其餘 7）
            assert conn.execute("SELECT COUNT(*) FROM occurrences").fetchone()[0] == 17
        finally:
            conn.close()

    def test_schema_mismatch_triggers_rebuild(
        self, tmp_path, monkeypatch, capsys
    ) -> None:
        """schema 守衛：valid sqlite 但舊 schema——放行會到查詢時才 crash
        （no such column），視同過期重建即治。"""
        idx_file = write_index_file(tmp_path)
        db = scip_refs.sqlite_path(idx_file)
        scip_refs._build_db(FakeIndex([]), db, "")
        conn = sqlite3.connect(db)
        conn.execute("UPDATE meta SET value = '0' WHERE key = 'schema'")
        conn.commit()
        conn.close()
        monkeypatch.setattr(scip_refs, "load_index", lambda p: rich_index())
        assert isinstance(scip_refs.open_face(idx_file), scip_refs.SqliteFace)
        assert "schema 版本不符" in capsys.readouterr().err

    def test_rebuild_failure_falls_back_protobuf(
        self, tmp_path, monkeypatch, capsys
    ) -> None:
        idx_file = write_index_file(tmp_path)
        db = scip_refs.sqlite_path(idx_file)
        scip_refs._build_db(FakeIndex([]), db, "")
        older = idx_file.stat().st_mtime - 100
        os.utime(db, (older, older))

        def boom(index, db_path, head):
            raise OSError("disk full")

        monkeypatch.setattr(scip_refs, "_build_db", boom)
        parses: list[Path] = []
        monkeypatch.setattr(
            scip_refs,
            "load_index",
            lambda p: (parses.append(p), FakeIndex([]))[1],
        )
        assert isinstance(scip_refs.open_face(idx_file), scip_refs.ProtobufFace)
        assert "重建失敗" in capsys.readouterr().err
        # 審查 F4：解析一次留存——失敗 fallback 不得二次解析
        assert parses == [idx_file]


class TestByteIdentity:
    """②驗收級——同底稿 protobuf 路徑與 sqlite 路徑的 main() stdout
    **逐位相同**（含 [SRC] 缺席、...共 N 處 行、? 行）。"""

    def _run(self, monkeypatch, argv: list[str]) -> int:
        monkeypatch.setattr(sys, "argv", ["scip_refs", *argv])
        return main()

    def test_query_stdout_identical(self, tmp_path, monkeypatch, capsys) -> None:
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        idx_file = write_index_file(tmp_path)
        monkeypatch.setattr(scip_refs, "load_index", lambda p: rich_index())
        argv = ["EventStoreLifecycle.open", "--index", str(idx_file)]
        rc1 = self._run(monkeypatch, argv)
        out1 = capsys.readouterr().out
        assert rc1 == 0
        assert self._run(monkeypatch, ["--build-cache", "--index", str(idx_file)]) == 0
        capsys.readouterr()
        # db 在——protobuf 解析不該被觸（路由釘住）
        monkeypatch.setattr(
            scip_refs,
            "load_index",
            lambda p: (_ for _ in ()).throw(AssertionError("應走 sqlite")),
        )
        rc2 = self._run(monkeypatch, argv)
        assert rc2 == 0
        assert capsys.readouterr().out == out1

    def test_bare_query_stdout_identical(self, tmp_path, monkeypatch, capsys) -> None:
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        idx_file = write_index_file(tmp_path)
        monkeypatch.setattr(scip_refs, "load_index", lambda p: rich_index())
        argv = ["open", "--index", str(idx_file)]
        rc1 = self._run(monkeypatch, argv)
        out1 = capsys.readouterr().out
        assert rc1 == 0
        assert self._run(monkeypatch, ["--build-cache", "--index", str(idx_file)]) == 0
        capsys.readouterr()
        rc2 = self._run(monkeypatch, argv)
        assert rc2 == 0
        assert capsys.readouterr().out == out1

    def test_no_match_stdout_identical(self, tmp_path, monkeypatch, capsys) -> None:
        """審查 F6a：exit 1（查無 DEF）路徑的 stdout 位元組相同。"""
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        idx_file = write_index_file(tmp_path)
        monkeypatch.setattr(scip_refs, "load_index", lambda p: rich_index())
        argv = ["RefOnly.run", "--index", str(idx_file)]  # ref-only——無 DEF
        rc1 = self._run(monkeypatch, argv)
        out1 = capsys.readouterr().out
        assert rc1 == 1
        assert self._run(monkeypatch, ["--build-cache", "--index", str(idx_file)]) == 0
        capsys.readouterr()
        rc2 = self._run(monkeypatch, argv)
        assert rc2 == 1
        assert capsys.readouterr().out == out1

    def test_stale_db_rebuild_in_main_keeps_stdout(
        self, tmp_path, monkeypatch, capsys
    ) -> None:
        """審查 F6b：main() 查詢內自動重建——WARN 走 stderr，stdout 不變。"""
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        idx_file = write_index_file(tmp_path)
        monkeypatch.setattr(scip_refs, "load_index", lambda p: rich_index())
        argv = ["EventStoreLifecycle.open", "--index", str(idx_file)]
        rc1 = self._run(monkeypatch, argv)
        out1 = capsys.readouterr().out
        assert rc1 == 0
        assert self._run(monkeypatch, ["--build-cache", "--index", str(idx_file)]) == 0
        capsys.readouterr()
        db = scip_refs.sqlite_path(idx_file)
        older = idx_file.stat().st_mtime - 100
        os.utime(db, (older, older))
        rc2 = self._run(monkeypatch, argv)
        captured = capsys.readouterr()
        assert rc2 == 0
        assert captured.out == out1
        assert "自動重建" in captured.err

    def test_audit_stdout_identical(self, tmp_path, monkeypatch, capsys) -> None:
        monkeypatch.setattr(scip_refs, "scip_pb2", object())
        monkeypatch.setattr(scip_refs, "_git_head", lambda repo: None)
        repo = tmp_path / "repo"
        repo.mkdir()
        idx_file = write_index_file(tmp_path)
        missing_json = json.dumps(
            {
                "missing": [
                    {
                        "file": str(repo / "crates/a.rs"),
                        "symbol": "open",
                        "db_count": 0,
                        "ra_count": 1,
                    }
                ]
            }
        )
        proc = SimpleNamespace(returncode=0, stdout=missing_json, stderr="")
        monkeypatch.setattr(scip_refs.subprocess, "run", lambda *a, **kw: proc)
        monkeypatch.setattr(scip_refs, "load_index", lambda p: rich_index())
        argv = ["--audit", "--repo", str(repo), "--index", str(idx_file)]
        rc1 = self._run(monkeypatch, argv)
        out1 = capsys.readouterr().out
        assert rc1 == 0
        assert self._run(monkeypatch, ["--build-cache", "--index", str(idx_file)]) == 0
        capsys.readouterr()
        rc2 = self._run(monkeypatch, argv)
        assert rc2 == 0
        assert capsys.readouterr().out == out1

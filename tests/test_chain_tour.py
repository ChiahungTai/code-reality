"""S2 chain_tour 單元測試——callchain 文檔 → 每場景一條 CodeTour（SM-4/5/6）。

解析機械搬運自 .agent-tmp/ui/chain_viewer.py（187 幀/L6 驗證源；html viewer
已退役不搬）。此處釘：樹狀解析（stack depth 修正 pl//3 失真）、GraphAnchor
五態、tour 映射規則（重錨優先/skip 記數/樹狀前綴 title）、SM-6 crash。
"""

import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

import pytest
from crg_db import make_crg_db, qualified
from profile_repo import write_mosaic_profile

from code_reality.chain_tour import (
    GraphAnchor,
    PathResolver,
    ScenarioTours,
    _step_of,
    best_ident,
    build_tours,
    check_anchor,
    main,
    parse_blocks,
    parse_frames,
    write_tours,
)
from code_reality.common import anchor_pattern

FENCE = "```"


class TestParse:
    def test_blocks_only_tree_lines(self) -> None:
        md = f"# Chain\n## 場景 A\n{FENCE}\nroot\n└─ main()  a.py:1\n{FENCE}\n## 純代碼（非 callstack）\n{FENCE}\nprint('no tree')\n{FENCE}"
        blocks = parse_blocks(md)
        assert len(blocks) == 1
        assert blocks[0]["heading"] == "場景 A"

    def test_frames_depth_stack_derived(self) -> None:
        """EP 已知限制修正：`│ +4 空格` 縮排用 pl//3 會跳層失真——stack 推導。"""
        block = {
            "heading": "t",
            "lines": [
                "root  a.py:1",
                "│ ├─ mid()  a.py:2",
                "│    └─ deep()  a.py:3",
                "│ ├─ mid2()  a.py:4",
            ],
        }
        frames = parse_frames(block)
        assert [f["depth"] for f in frames] == [0, 1, 2, 1]
        assert frames[2]["parent"] == 1
        assert frames[3]["parent"] == 0

    def test_frame_ref_note_split(self) -> None:
        block = {
            "heading": "t",
            "lines": [
                "   └─ MarketService().is_trading_day()  services/_market.py:41"
                "   # plist_mode 休市 skip",
            ],
        }
        f = parse_frames(block)[0]
        assert f["path"] == "services/_market.py"
        assert f["line"] == 41
        assert "休市" in f["note"]
        assert f["symbol"].startswith("MarketService")

    def test_frames_rs_ref_pyi_excluded(self) -> None:
        """F1（NT dogfood）：.rs 幀錨與 .py 同構；.pyi stub 不入錨（宣告層）。"""
        block = {
            "heading": "t",
            "lines": [
                "root  crates/engine/src/kernel.rs:42",
                "└─ Engine::run()  crates/engine/src/engine.rs:120  # 主循環",
                "└─ LiveNode  live/node.pyi:30",
            ],
        }
        frames = parse_frames(block)
        assert frames[0]["path"] == "crates/engine/src/kernel.rs"
        assert frames[0]["line"] == 42
        assert frames[1]["path"] == "crates/engine/src/engine.rs"
        assert frames[1]["line"] == 120
        assert "主循環" in frames[1]["note"]
        assert frames[2]["path"] is None  # .pyi 無錨 → noref skip 分類

    def test_best_ident(self) -> None:
        assert best_ident("main()") == "main"
        assert (
            best_ident("TradingHost.should_skip_for_holiday()")
            == "should_skip_for_holiday"
        )
        # F8（NT dogfood）：`::` 裸寫分裂多 candidates 走 longest-wins——
        # struct 名（通常更長）搶走 ident＝錨到 struct 定義行的根因；`()` 呼叫狀優先導正
        assert best_ident("EventStoreLifecycle::open") == "EventStoreLifecycle"
        assert best_ident("EventStoreLifecycle::open()") == "open"


class TestResolve:
    def _repo(self, tmp_path: Path, files: list[str]) -> Path:
        repo = tmp_path / "repo"
        for rel in files:
            p = repo / "mosaic_alpha" / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text("x\n")
        write_mosaic_profile(repo)
        return repo

    def test_direct_and_suffix(self, tmp_path: Path) -> None:
        repo = self._repo(tmp_path, ["apps/run.py"])
        r = PathResolver(repo)
        assert r.resolve("apps/run.py")[1] == "direct"
        got, kind = r.resolve("run.py")
        assert kind == "suffix"
        assert got is not None and got.name == "run.py"

    def test_ambiguous_and_none(self, tmp_path: Path) -> None:
        repo = self._repo(tmp_path, ["a/x.py", "b/x.py"])
        r = PathResolver(repo)
        assert r.resolve("x.py") == (None, "ambiguous")
        assert r.resolve("nope.py") == (None, "none")

    def test_ctx_prefers_bumped_dir(self, tmp_path: Path) -> None:
        repo = self._repo(tmp_path, ["apps/run.py", "apps/x.py", "b/x.py"])
        r = PathResolver(repo)
        assert r.resolve("run.py")[1] == "suffix"  # bump mosaic_alpha/apps
        got, kind = r.resolve("x.py")
        assert kind == "ctx"
        assert got is not None and got.parent.name == "apps"

    def test_suffix_boundary_excludes_partial_name(self, tmp_path: Path) -> None:
        """`xrun.py` 不得撞名 `run.py`——/ 邊界讓真 run.py 唯一解析。"""
        repo = self._repo(tmp_path, ["apps/run.py", "xrun.py"])
        r = PathResolver(repo)
        got, kind = r.resolve("run.py")
        assert kind == "suffix"
        assert got is not None and got.parent.name == "apps"

    def test_no_profile_fallback_root_direct(self, tmp_path: Path) -> None:
        """無 profile repo：generic fallback pkg_roots=[repo_root]——根檔 direct 命中。"""
        repo = tmp_path / "bare"
        (repo / "src").mkdir(parents=True)
        (repo / "src" / "app.py").write_text("x\n")
        r = PathResolver(repo)
        got, kind = r.resolve("src/app.py")
        assert kind == "direct"
        assert got is not None and got.name == "app.py"

    def test_venv_same_name_excluded_from_pool(self, tmp_path: Path) -> None:
        """.venv/ 同名檔不得進 pool——排除前綴讓真 package 檔從 ambiguous 變唯一。"""
        repo = tmp_path / "repo"
        for rel in ("mosaic_alpha/apps/x.py", ".venv/lib/x.py"):
            p = repo / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text("x\n")
        write_mosaic_profile(repo)
        r = PathResolver(repo)
        got, kind = r.resolve("x.py")
        assert kind == "suffix"
        assert got is not None and got.parent.name == "apps"


class TestCheckAnchor:
    def test_statuses(self, tmp_path: Path) -> None:
        # foo 只在第 2 行；filler 拉長檔案讓 ±8 窗外仍有「檔內他處」可判 drift-far
        content = "l1\ndef foo():\n" + "".join(f"filler{i}\n" for i in range(3, 20))
        f = tmp_path / "m.py"
        f.write_text(content)
        assert check_anchor(f, 2, "foo") == "ok"
        assert check_anchor(f, 5, "foo") == "drift"
        assert check_anchor(f, 18, "foo") == "drift-far"
        assert check_anchor(f, 2, "bar") == "missing"
        assert check_anchor(f, 2, "") == "nocheck"


class TestGraphAnchor:
    def _db(self, tmp_path: Path) -> tuple[Path, Path]:
        repo = (tmp_path / "repo").resolve()
        repo.mkdir()
        db = tmp_path / "graph.db"
        x, y, z = (
            qualified(repo, "mosaic_alpha/x.py", "X.func"),
            qualified(repo, "mosaic_alpha/y.py", "Y.relocated"),
            qualified(repo, "mosaic_alpha/z.py", "Z.relocated"),
        )
        make_crg_db(
            db,
            nodes=[
                ("func", None, x, str(repo / "mosaic_alpha/x.py")),
                ("relocated", None, y, str(repo / "mosaic_alpha/y.py")),
                ("relocated", None, z, str(repo / "mosaic_alpha/z.py")),
            ],
            node_lines={x: 100, y: 50, z: 60},
        )
        return db, repo

    def test_same_and_moved(self, tmp_path: Path) -> None:
        db, repo = self._db(tmp_path)
        ga = GraphAnchor(db, repo)
        x = repo / "mosaic_alpha/x.py"
        assert ga.anchor(x, 100, "func")["g"] == "same"
        m = ga.anchor(x, 95, "func")
        assert m["g"] == "moved" and m["g_line"] == 100 and m["g_delta"] == 5

    def test_moved_file_ambiguous(self, tmp_path: Path) -> None:
        db, repo = self._db(tmp_path)
        ga = GraphAnchor(db, repo)
        x = repo / "mosaic_alpha/x.py"
        r = ga.anchor(x, 10, "relocated", "missing")
        assert r["g"] == "moved-file-ambiguous"  # y+z 兩檔同名

    def test_moved_file_unique(self, tmp_path: Path) -> None:
        """跨檔唯一命中 → moved-file 指新檔（SM-5 的單元級案例）。"""
        repo = (tmp_path / "repo1").resolve()
        repo.mkdir()
        (repo / "mosaic_alpha").mkdir(parents=True)
        db = tmp_path / "graph1.db"
        y = qualified(repo, "mosaic_alpha/y.py", "Y.relocated")
        make_crg_db(
            db,
            nodes=[("relocated", None, y, str(repo / "mosaic_alpha/y.py"))],
            node_lines={y: 50},
        )
        ga = GraphAnchor(db, repo)
        r = ga.anchor(repo / "mosaic_alpha/x.py", 10, "relocated", "missing")
        assert r["g"] == "moved-file"
        assert r["g_file"] == "mosaic_alpha/y.py"
        assert r["g_line"] == 50

    def test_null_line_start_not_shadowing(self, tmp_path: Path) -> None:
        """同 (file, name) 的 NULL line_start 列不得陰影有效列（ASC NULL 排最前）。"""
        repo = (tmp_path / "repo2").resolve()
        repo.mkdir()
        (repo / "mosaic_alpha").mkdir(parents=True)
        db = tmp_path / "graph2.db"
        x_ok = qualified(repo, "mosaic_alpha/x.py", "X.func")
        x_null = qualified(repo, "mosaic_alpha/x.py", "X.funcNull")
        make_crg_db(
            db,
            nodes=[
                ("func", None, x_ok, str(repo / "mosaic_alpha/x.py")),
                ("func", None, x_null, str(repo / "mosaic_alpha/x.py")),
            ],
            node_lines={x_ok: 100},  # x_null 保持 line_start NULL
        )
        ga = GraphAnchor(db, repo)
        assert ga.anchor(repo / "mosaic_alpha/x.py", 100, "func")["g"] == "same"

    def test_not_in_graph_gates(self, tmp_path: Path) -> None:
        db, repo = self._db(tmp_path)
        ga = GraphAnchor(db, repo)
        x = repo / "mosaic_alpha/x.py"
        # substring 非 missing → 不跨檔猜
        assert ga.anchor(x, 10, "relocated", "ok")["g"] == "not-in-graph"
        # ident 太短 → 不跨檔猜（撞名噪音門檻）
        assert ga.anchor(x, 10, "ab", "missing")["g"] == "not-in-graph"


class TestBuildTours:
    def _setup(self, tmp_path: Path) -> tuple[Path, Path, Path]:
        repo = (tmp_path / "repo").resolve()
        pkg = repo / "mosaic_alpha"
        pkg.mkdir(parents=True)
        (pkg / "app.py").write_text("def main():\n    pass\n")
        (pkg / "util.py").write_text("x\n")  # 不得含 helper——substring 須判 missing
        (pkg / "plain.py").write_text("plain\n")
        (pkg / "newhome.py").write_text("def helper():\n    pass\n")
        db = tmp_path / "graph.db"
        q_app = qualified(repo, "mosaic_alpha/app.py", "main")
        q_plain = qualified(repo, "mosaic_alpha/plain.py", "plain")
        q_new = qualified(repo, "mosaic_alpha/newhome.py", "helper")
        make_crg_db(
            db,
            nodes=[
                ("main", None, q_app, str(pkg / "app.py")),
                ("plain", None, q_plain, str(pkg / "plain.py")),
                ("helper", None, q_new, str(pkg / "newhome.py")),
            ],
            node_lines={q_app: 40, q_plain: 1, q_new: 20},
        )
        md = tmp_path / "chain.md"
        md.write_text(
            f"# Chain\n## 場景 X\n{FENCE}\nroot\n└─ main()  app.py:10\n   ├─ helper()  util.py:5\n   ├─ shell wrapper（無錨）\n   └─ plain()  plain.py:1\n{FENCE}"
        )
        return md, repo, db

    def test_tour_mapping_rules(self, tmp_path: Path) -> None:
        """SM-4 核心：重錨優先行號、moved-file 指新檔、skip 記數、樹狀前綴。"""
        md, repo, db = self._setup(tmp_path)
        st = build_tours(md, repo, db)
        assert len(st.tours) == 1
        tour = st.tours[0]
        assert tour["title"] == "場景 X"
        assert st.frames == 5 and st.skipped == 2
        steps = tour["steps"]
        assert len(steps) == 3

        s_main, s_helper, s_plain = steps
        assert s_main["file"] == "mosaic_alpha/app.py"
        assert s_main["line"] == 40  # graph 重錨優先（文檔錨 :10）
        assert "graph +30 → :40" in s_main["description"]
        assert "└─" in s_main["title"] and "main()" in s_main["title"]

        # moved-file：步指向新檔＋描述記搬家（SM-5 同型）
        assert s_helper["file"] == "mosaic_alpha/newhome.py"
        assert s_helper["line"] == 20
        assert "搬家" in s_helper["description"]
        assert "util.py:5" in s_helper["description"]

        assert s_plain["file"] == "mosaic_alpha/plain.py"
        assert s_plain["line"] == 1
        assert "2 幀跳過" in tour["description"] and "noref 2" in tour["description"]

    def test_no_graph_still_tours(self, tmp_path: Path) -> None:
        """無 graph.db（--graph 未給）→ 純文檔錨，重錨退化但不擋。"""
        md, repo, _ = self._setup(tmp_path)
        st = build_tours(md, repo, None)
        assert st.tours[0]["steps"][0]["line"] == 10  # 文檔錨原值

    def test_rs_frame_pipeline_anchored(self, tmp_path: Path) -> None:
        """F1 整合：.rs 幀 resolve＋check_anchor＋graph 重錨全管線（Rust repo 形態）；
        無錨 root 幀仍走 noref skip——分類不因副檔擴充改變。"""
        repo = (tmp_path / "repo").resolve()
        src = repo / "crates/engine/src"
        src.mkdir(parents=True)
        (src / "engine.rs").write_text("pub fn run() {\n}\n")
        db = tmp_path / "graph.db"
        q = qualified(repo, "crates/engine/src/engine.rs", "run")
        make_crg_db(
            db,
            nodes=[("run", None, q, str(src / "engine.rs"))],
            node_lines={q: 2},
        )
        md = tmp_path / "chain.md"
        md.write_text(
            f"# Chain\n## 場景 R\n{FENCE}\nroot\n└─ run()  crates/engine/src/engine.rs:1\n{FENCE}"
        )
        st = build_tours(md, repo, db)
        assert st.frames == 2 and st.skipped == 1  # root 無錨跳過、.rs 幀不跳
        assert st.g_counts == {"noref": 1, "moved": 1}
        step = st.tours[0]["steps"][0]
        assert step["file"] == "crates/engine/src/engine.rs"
        assert step["line"] == 2  # graph 重錨優先（文檔錨 :1、g moved +1）

    def test_write_tours_filenames(self, tmp_path: Path) -> None:
        """寫檔段：{NN}.tour 純序號（user 裁定——族名承載語義、檔名穩定鍵；zero-pad 防字典序亂調）。"""
        md, repo, db = self._setup(tmp_path)
        st = build_tours(md, repo, db)
        paths = write_tours(st, tmp_path / "out")
        assert len(paths) == 1
        assert paths[0].name == "01.tour"
        assert paths[0].exists()

    def test_write_tours_warns_legacy_filenames(
        self, tmp_path: Path, capsys: pytest.CaptureFixture
    ) -> None:
        """D-f 過渡：out_dir 殘留舊格式 chain-*.tour → [WARN] 提示清理
        （防新舊同 title 並存、player 撞鍵靜默雙份）。"""
        md, repo, db = self._setup(tmp_path)
        st = build_tours(md, repo, db)
        out = tmp_path / "out"
        out.mkdir()
        (out / "chain-01-舊格式.tour").write_text("{}", encoding="utf-8")
        write_tours(st, out)
        assert "舊檔名格式殘留" in capsys.readouterr().out

    def test_written_title_nn_prefix_upstream_parseable(self, tmp_path: Path) -> None:
        """tour-contract EP S2（SM-4）——寫檔後 title 帶 ``NN - `` 前綴（記憶體
        raw heading 不變——防 01-01 雙重編號）；上游連鎖
        regex ``^#?(\\d+)\\s+-`` 逐條可解析。"""
        md, repo, db = self._setup(tmp_path)
        st = build_tours(md, repo, db)
        assert st.tours[0]["title"] == "場景 X"  # 記憶體 title 不帶前綴
        paths = write_tours(st, tmp_path / "out")
        written = json.loads(paths[0].read_text())
        assert written["title"] == "01 - 場景 X"
        m = re.match(r"^#?(\d+)\s+-", written["title"])
        assert m and m.group(1) == "01"


class TestWriteToursPrimary:
    """tour-contract EP S2（SM-5）——isPrimary 顯式旗標（補零 corpus 唯一有效
    primary 機制；預設不標＝user 08-22 裁決）。"""

    def _st(self) -> ScenarioTours:
        return ScenarioTours(
            tours=[
                {"title": "場景 一", "description": "d", "steps": []},
                {"title": "場景 二", "description": "d", "steps": []},
            ],
            frames=0,
            skipped=0,
            g_counts={},
        )

    def test_primary_marks_selected_only(self, tmp_path: Path) -> None:
        paths = write_tours(self._st(), tmp_path / "out", primary={2})
        t1 = json.loads(paths[0].read_text())
        t2 = json.loads(paths[1].read_text())
        assert "isPrimary" not in t1
        assert t2["isPrimary"] is True
        assert t2["title"] == "02 - 場景 二"

    def test_no_primary_by_default(self, tmp_path: Path) -> None:
        paths = write_tours(self._st(), tmp_path / "out")
        for p in paths:
            assert "isPrimary" not in json.loads(p.read_text())


class TestPatternEmission:
    """tour-contract EP S1（SM-3）——pattern 取重錨後新行內容（非文檔錨舊行）。"""

    def _setup(self, tmp_path: Path) -> tuple[Path, Path, Path, Path]:
        repo = (tmp_path / "repo").resolve()
        pkg = repo / "mosaic_alpha"
        pkg.mkdir(parents=True)
        # 重錨行須有真內容：app.py 第 40 行＝def main():、newhome.py 第 20 行＝def helper():
        (pkg / "app.py").write_text(
            "".join(f"filler{i}\n" for i in range(1, 40)) + "def main():\n    pass\n"
        )
        (pkg / "newhome.py").write_text(
            "".join(f"pad{i}\n" for i in range(1, 20)) + "def helper():\n    pass\n"
        )
        (pkg / "util.py").write_text("x\n")  # 不得含 helper——substring 須判 missing
        (pkg / "blank.py").write_text("a\nb\n\nc\n")  # 第 3 行空行
        db = tmp_path / "graph.db"
        q_app = qualified(repo, "mosaic_alpha/app.py", "main")
        q_new = qualified(repo, "mosaic_alpha/newhome.py", "helper")
        make_crg_db(
            db,
            nodes=[
                ("main", None, q_app, str(pkg / "app.py")),
                ("helper", None, q_new, str(pkg / "newhome.py")),
            ],
            node_lines={q_app: 40, q_new: 20},
        )
        md = tmp_path / "chain.md"
        md.write_text(
            f"# Chain\n## 場景 P\n{FENCE}\nroot\n└─ main()  app.py:10\n   ├─ helper()  util.py:5\n   └─ blankfn()  blank.py:3\n{FENCE}"
        )
        return md, repo, db, pkg

    def test_moved_pattern_from_reanchored_line(self, tmp_path: Path) -> None:
        """SM-3：moved 幀 pattern＝重錨後行 40 內容（def main():），非文檔錨行 10。"""
        md, repo, db, pkg = self._setup(tmp_path)
        st = build_tours(md, repo, db)
        s_main, _s_helper, _s_blank = st.tours[0]["steps"]
        assert s_main["pattern"] == anchor_pattern("def main():")
        assert re.search(s_main["pattern"], (pkg / "app.py").read_text(), re.MULTILINE)

    def test_moved_file_pattern_from_new_file_line(self, tmp_path: Path) -> None:
        """SM-3 同型：moved-file 幀 pattern 取新檔 g_line=20 行內容。"""
        md, repo, db, _pkg = self._setup(tmp_path)
        st = build_tours(md, repo, db)
        _, s_helper, _ = st.tours[0]["steps"]
        assert s_helper["pattern"] == anchor_pattern("def helper():")

    def test_blank_anchor_line_no_pattern(self, tmp_path: Path) -> None:
        """省略條件①：錨行 strip 後為空 → 不發射（`^\\s*` 零寬每行匹配會錯 line 1）。"""
        md, repo, db, _ = self._setup(tmp_path)
        st = build_tours(md, repo, db)
        _, _, s_blank = st.tours[0]["steps"]
        assert s_blank["file"] == "mosaic_alpha/blank.py"
        assert s_blank["line"] == 3
        assert "pattern" not in s_blank

    def test_moved_file_target_missing_no_crash_no_pattern(
        self, tmp_path: Path
    ) -> None:
        """re-review F2：graph 宣稱的搬家目標檔不存在 → 不發射 pattern，
        非 FileNotFoundError 殺全場（graph 世代落後的真實風險）。"""
        repo = (tmp_path / "repo").resolve()
        pkg = repo / "mosaic_alpha"
        pkg.mkdir(parents=True)
        (pkg / "util.py").write_text("x\n")
        f: dict[str, Any] = {
            "abs_path": pkg / "util.py",
            "path": "util.py",
            "line": 5,
            "g": "moved-file",
            "g_file": "mosaic_alpha/gone.py",  # graph 宣稱新家——已不存在
            "g_line": 3,
            "prefix": "└─ ",
            "symbol": "helper()",
            "note": "",
        }
        step = _step_of(f, repo, {})
        assert step["file"] == "mosaic_alpha/gone.py"
        assert "pattern" not in step


class TestCli:
    def test_bad_format_crashes_with_guidance(self, tmp_path: Path) -> None:
        """SM-6：非 callstack 格式 → crash-only 附指引。"""
        md = tmp_path / "bad.md"
        md.write_text(f"# not chain\n\n{FENCE}\nplain code\n{FENCE}\n")
        r = subprocess.run(
            [
                sys.executable,
                "-m",
                "code_reality.chain_tour",
                str(md),
                "--repo",
                str(tmp_path),
                "--out-dir",
                str(tmp_path / "o"),
            ],
            cwd=tmp_path,
            capture_output=True,
            text=True,
            check=False,
        )
        assert r.returncode != 0
        assert "callstack" in r.stderr

    def test_default_subdir_and_primary_flag(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """tour-contract EP S2（SM-5/8，EP review F7）——CLI 級：預設
        ``.tours/arch/<md-stem>/`` 子目錄＋``--primary`` 旗標寫入 isPrimary。
        in-process main()＋chdir：預設路徑相對 cwd，落 tmp 不污真 .tours/。"""
        md = tmp_path / "paper-chain-鏈.md"  # stem 非 ASCII 保留（F10 對照語義）
        # 幀刻意指向 tmp repo 不存在的檔（全 skip、tour 0 步）——本測只釘
        # 預設路徑與 primary 接線，不測幀解析
        md.write_text(
            f"# Chain\n## 場景 一\n{FENCE}\nroot\n└─ main()  app.py:1\n{FENCE}\n## 場景 二\n{FENCE}\nroot\n└─ other()  b.py:1\n{FENCE}"
        )
        monkeypatch.chdir(tmp_path)
        monkeypatch.setattr(sys, "argv", ["chain_tour", str(md), "--primary", "2"])
        main()
        base = tmp_path / ".tours" / "arch" / "paper-chain-鏈"
        p1, p2 = sorted(base.glob("*.tour"))
        t1, t2 = json.loads(p1.read_text()), json.loads(p2.read_text())
        assert t1["title"] == "01 - 場景 一"
        assert t2["title"] == "02 - 場景 二"
        assert "isPrimary" not in t1
        assert t2["isPrimary"] is True

    def test_primary_out_of_range_crashes_loud(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """re-review F2：--primary 越界（場景不存在）→ crash 大聲非靜默 no-op。"""
        md = tmp_path / "chain.md"
        md.write_text(
            f"# Chain\n\n## 場景 一\n\n{FENCE}\nroot\n└─ main()  app.py:1\n{FENCE}\n"
        )
        monkeypatch.chdir(tmp_path)
        monkeypatch.setattr(sys, "argv", ["chain_tour", str(md), "--primary", "9"])
        with pytest.raises(AssertionError, match="越界"):
            main()


def test_main_outdir_outside_tours_skips_manifest(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture
) -> None:
    """M2 D-b：out-dir 不在 .tours/ 樹內（dry-run 暫存）→ tour 照寫、manifest 零副作用。"""
    md = tmp_path / "chain.md"
    md.write_text(
        f"# Chain\n\n## 場景 一\n\n{FENCE}\nroot\n└─ main()  app.py:1\n{FENCE}\n"
    )
    dry = tmp_path / "agent-tmp" / "chain-dry"
    monkeypatch.chdir(tmp_path)
    monkeypatch.setattr(
        sys,
        "argv",
        ["chain_tour", str(md), "--repo", str(tmp_path), "--out-dir", str(dry)],
    )
    main()
    assert list(dry.glob("*.tour")), "tour 檔照寫"
    assert not list(tmp_path.rglob("manifest.toml")), "dry 目錄不得產生 manifest 副作用"
    out_text = capsys.readouterr().out
    assert "manifest skip" in out_text
    assert "manifest upsert" not in out_text, "guard 誤判走 else 的 regression"


def test_main_outdir_is_tours_root_itself(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture
) -> None:
    """guard 最小正邊界：out-dir＝.tours 本身（tours_root_of 零步迴圈）→ upsert 照常。"""
    md = tmp_path / "chain.md"
    md.write_text(
        f"# Chain\n\n## 場景 一\n\n{FENCE}\nroot\n└─ main()  app.py:1\n{FENCE}\n"
    )
    out = tmp_path / ".tours"
    monkeypatch.chdir(tmp_path)
    monkeypatch.setattr(
        sys,
        "argv",
        ["chain_tour", str(md), "--repo", str(tmp_path), "--out-dir", str(out)],
    )
    main()
    assert (out / "manifest.toml").exists()
    assert "manifest upsert" in capsys.readouterr().out


def test_main_outdir_inside_tours_upserts_manifest(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture
) -> None:
    """M1 行為回歸守衛：out-dir 在 .tours/ 樹內 → manifest upsert 照常。"""
    md = tmp_path / "chain.md"
    md.write_text(
        f"# Chain\n\n## 場景 一\n\n{FENCE}\nroot\n└─ main()  app.py:1\n{FENCE}\n"
    )
    out = tmp_path / ".tours" / "arch" / "x"
    monkeypatch.chdir(tmp_path)
    monkeypatch.setattr(
        sys,
        "argv",
        ["chain_tour", str(md), "--repo", str(tmp_path), "--out-dir", str(out)],
    )
    main()
    assert (tmp_path / ".tours" / "manifest.toml").exists()
    assert "manifest upsert" in capsys.readouterr().out

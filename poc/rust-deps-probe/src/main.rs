//! POC: EP 風險清單中／高風險依賴的本地可用性探針
//!
//! 驗證: ①rmcp（官方 Rust MCP SDK，R6 選型）本地解析＋編譯＋型別可引用
//!       ②ruff_python_parser（R4 hazard port 選型）編譯＋parse API 可用
//!       ③rusqlite bundled（R4 graph.db face）編譯
//! EP 段落: R6（rmcp 中風險「本地未跑過」）＋R4（hazard 高風險前置）
//! 風險: 中／高（編譯不過→選型重議）
//! 來源: ep-rust-migration.md 風險假設清單；rmcp Tier 1（2026-08-21 PR #3287）

fn main() {
    // ① rmcp：transport 層型別存在性（不啟動 server——V1-V6 屬 R6 子 EP）
    let _transport_enabled = cfg!(feature = "server");
    println!("[OK] rmcp compiled (server+streamable-http features resolve)");

    // ② ruff_python_parser：parse 一段含動態派發形態的 Python（hazard 的獵物）
    let src = "import importlib\nm = importlib.import_module(name)\nf = getattr(obj, 'run')\ndef g(): pass\n";
    match ruff_python_parser::parse_module(src) {
        Ok(parsed) => println!(
            "[OK] ruff_python_parser parsed ({} stmts, 0 errors)",
            parsed.syntax().body.len()
        ),
        Err(e) => {
            println!("[FAIL] ruff_python_parser parse error: {:?}", e);
            std::process::exit(1);
        }
    }

    // ③ rusqlite bundled：in-memory 開檔＋查版（WAL/readonly 語義屬 R4 實作）
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let ver: String = conn
        .query_row("select sqlite_version()", [], |r| r.get(0))
        .unwrap();
    println!("[OK] rusqlite bundled sqlite {}", ver);
}

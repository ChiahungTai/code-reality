//! `graph_csv` — the frozen `code_reality/graph_csv.py` contract: CRG
//! graph.db → nodes/links CSV (Cosmograph feed). File nodes carry no
//! community (Leiden runs at function/class level) — file-level
//! ownership is the member-majority vote with the `(-count, id)` tie
//! break. CSV bytes: Python `csv.writer` excel dialect — QUOTE_MINIMAL
//! and the CRLF line terminator (the `\n` vs `\r\n` split would silently
//! pass a stdout-only gate, so the file bytes are part of the parity face).

use crate::argparse::{parse, FlagSpec, Kind, Outcome, ToolSpec};
use crate::common::{assert_db_unchanged, connect_ro, db_mtime_ns, graph_db_path, EDGE_KINDS};
use crate::profile::{is_excluded, load_profile};
use crate::ToolOutput;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const SPEC: ToolSpec = ToolSpec {
    flags: &[
        FlagSpec {
            long: "--repo",
            short: None,
            kind: Kind::Value { metavar: "REPO" },
        },
        FlagSpec {
            long: "--out-dir",
            short: None,
            kind: Kind::Value { metavar: "OUT_DIR" },
        },
    ],
    positionals: &[],
};

const HELP: &str = concat!(
    "usage: graph_csv [-h] [--repo REPO] [--out-dir OUT_DIR]\n",
    "\n",
    "graph CSV export——CRG graph.db → nodes/links CSV（Cosmograph 餵料）。\n",
    "\n",
    "options:\n",
    "  -h, --help         show this help message and exit\n",
    "  --repo REPO        repo 根（含 .code-review-graph/）\n",
    "  --out-dir OUT_DIR  輸出目錄（預設 .agent-tmp/——CSV 是按需重產的玩圖資產）\n",
);

pub struct GraphCsv {
    pub nodes: Vec<NodeRow>,
    pub links: Vec<LinkRow>,
    pub communities: HashMap<i64, String>,
}

pub struct NodeRow {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub lang: String,
    pub is_test: bool,
    pub community: Option<i64>,
}

pub struct LinkRow {
    pub s: i64,
    pub t: i64,
    pub kinds: String,
}

/// graph.db → file-level nodes/links (`graph_csv.py:41-124`). Link pair
/// order = first-encounter order in the edge scan (Python dict insertion);
/// kinds aggregate as `"+".join(sorted)`.
pub fn load(db_path: &Path, repo_root: &Path) -> Result<GraphCsv, String> {
    let repo_root = crate::common::resolve(repo_root);
    let repo = format!("{}/", repo_root.display());
    let profile = load_profile(&repo_root)?;
    let m0 = db_mtime_ns(db_path)?;
    let conn = connect_ro(db_path)?;
    let out = load_with_conn(&conn, &repo_root, &repo, profile.as_ref());
    drop(conn);
    let graph = out?;
    assert_db_unchanged(db_path, m0)?;
    Ok(graph)
}

fn load_with_conn(
    conn: &rusqlite::Connection,
    repo_root: &Path,
    repo: &str,
    profile: Option<&crate::profile::Profile>,
) -> Result<GraphCsv, String> {
    // pass 1: qualified→file map + File-node ids
    let mut qual_file: HashMap<String, String> = HashMap::new();
    let mut file_ids: HashMap<String, i64> = HashMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT id, kind, qualified_name, file_path FROM nodes")
            .map_err(|e| format!("nodes 查詢失敗：{}", e))?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(|e| format!("nodes 查詢失敗：{}", e))?;
        for row in rows {
            let (nid, kind, qual, fp) = row.map_err(|e| format!("nodes 讀取失敗：{}", e))?;
            let fp = fp.unwrap_or_default();
            qual_file.insert(qual, fp.clone());
            if kind == "File" && !fp.is_empty() {
                file_ids.insert(fp, nid);
            }
        }
    }
    // community majority vote per file (non-File nodes with a community)
    let mut file_comm_votes: HashMap<String, HashMap<i64, i64>> = HashMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT file_path, community_id FROM nodes \
                 WHERE community_id IS NOT NULL AND kind != 'File'",
            )
            .map_err(|e| format!("community 投票查詢失敗：{}", e))?;
        let rows = stmt
            .query_map([], |r| {
                Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?))
            })
            .map_err(|e| format!("community 投票查詢失敗：{}", e))?;
        for row in rows {
            let (fp, cid) = row.map_err(|e| format!("community 讀取失敗：{}", e))?;
            if let Some(fp) = fp {
                *file_comm_votes
                    .entry(fp)
                    .or_default()
                    .entry(cid)
                    .or_insert(0) += 1;
            }
        }
    }
    let file_community = |fp: &str| -> Option<i64> {
        // tie-break: (-count, id) minimal — smallest id among the most voted
        file_comm_votes.get(fp).and_then(|votes| {
            votes
                .iter()
                .min_by(|a, b| (-a.1, a.0).cmp(&(-b.1, b.0)))
                .map(|(cid, _)| *cid)
        })
    };
    let keep = |fp: &str| -> bool {
        fp.strip_prefix(repo)
            .map(|rel| !is_excluded(rel, profile))
            .unwrap_or(false)
    };
    let mut nodes: Vec<NodeRow> = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT id, kind, name, file_path, language, is_test \
                 FROM nodes WHERE kind='File'",
            )
            .map_err(|e| format!("File 節點查詢失敗：{}", e))?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, i64>(5)?,
                ))
            })
            .map_err(|e| format!("File 節點查詢失敗：{}", e))?;
        for row in rows {
            let (id, name, fp, lang, is_test) =
                row.map_err(|e| format!("File 節點讀取失敗：{}", e))?;
            // Python `r[2]`/`r[3] or ""`: NULL name renders as an empty
            // CSV field, NULL file_path fails keep() — neither aborts
            let name = name.unwrap_or_default();
            let fp = fp.unwrap_or_default();
            if keep(&fp) {
                nodes.push(NodeRow {
                    id,
                    name,
                    path: fp[repo.len()..].to_string(),
                    lang: lang.unwrap_or_default(),
                    is_test: is_test != 0,
                    community: file_community(&fp),
                });
            }
        }
    }
    let file_set: std::collections::BTreeSet<String> =
        nodes.iter().map(|n| n.path.clone()).collect();
    let proj = |qual: &str| -> Option<String> {
        // Python truthiness: an EMPTY file_path mapping counts as absent
        // and falls through to the `::`-base branch
        let base = qual_file
            .get(qual)
            .filter(|f| !f.is_empty())
            .map(|f| f.as_str())
            .map_or_else(|| qual.split("::").next().unwrap_or(qual), |f| f);
        let stripped = base.strip_prefix(repo).unwrap_or(base);
        if file_set.contains(stripped) {
            Some(stripped.to_string())
        } else {
            None
        }
    };
    // pair aggregation preserving first-encounter order
    let mut pair_order: Vec<(i64, i64)> = Vec::new();
    let mut pair_kinds: HashMap<(i64, i64), std::collections::BTreeSet<String>> = HashMap::new();
    {
        let placeholders = "?1,?2,?3";
        let sql = format!(
            "SELECT kind, source_qualified, target_qualified FROM edges WHERE kind IN ({placeholders})"
        );
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("edges 查詢失敗：{}", e))?;
        let rows = stmt
            .query_map(EDGE_KINDS, |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| format!("edges 查詢失敗：{}", e))?;
        for row in rows {
            let (kind, sq, tq) = row.map_err(|e| format!("edges 讀取失敗：{}", e))?;
            let (Some(sp), Some(tp)) = (proj(&sq), proj(&tq)) else {
                continue;
            };
            if sp == tp {
                continue; // self-loop skip
            }
            let s = file_ids
                .get(&format!("{}/{sp}", repo_root.display()))
                .copied();
            let t = file_ids
                .get(&format!("{}/{tp}", repo_root.display()))
                .copied();
            let (Some(s), Some(t)) = (s, t) else {
                continue;
            };
            let entry = pair_kinds.entry((s, t)).or_default();
            if entry.is_empty() {
                pair_order.push((s, t));
            }
            entry.insert(kind);
        }
    }
    let links: Vec<LinkRow> = pair_order
        .into_iter()
        .map(|(s, t)| LinkRow {
            s,
            t,
            kinds: pair_kinds
                .get(&(s, t))
                .map(|k| {
                    let mut v: Vec<&str> = k.iter().map(|s| s.as_str()).collect();
                    v.sort();
                    v.join("+")
                })
                .unwrap_or_default(),
        })
        .collect();
    let mut communities: HashMap<i64, String> = HashMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT id, name FROM communities")
            .map_err(|e| format!("communities 查詢失敗：{}", e))?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| format!("communities 查詢失敗：{}", e))?;
        for row in rows {
            let (id, name) = row.map_err(|e| format!("communities 讀取失敗：{}", e))?;
            communities.insert(id, name);
        }
    }
    Ok(GraphCsv {
        nodes,
        links,
        communities,
    })
}

/// Undirected degrees; Σdegree == 2 × links invariant.
pub fn degrees(links: &[LinkRow]) -> HashMap<i64, i64> {
    let mut deg: HashMap<i64, i64> = HashMap::new();
    for e in links {
        *deg.entry(e.s).or_insert(0) += 1;
        *deg.entry(e.t).or_insert(0) += 1;
    }
    deg
}

/// QUOTE_MINIMAL single field (`graph_csv.py:136-163`): quote only on
/// `,` / `"` / CR / LF; embedded quotes double up.
fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\r') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// One CSV row WITHOUT the terminator (caller appends CRLF).
fn csv_row(fields: &[String]) -> String {
    fields
        .iter()
        .map(|f| csv_field(f))
        .collect::<Vec<_>>()
        .join(",")
}

pub fn write_csvs(g: &GraphCsv, out_dir: &Path) -> Result<(PathBuf, PathBuf), String> {
    std::fs::create_dir_all(out_dir)
        .map_err(|e| format!("{} 建立失敗：{}", out_dir.display(), e))?;
    let deg = degrees(&g.links);
    let nodes_path = out_dir.join("graph-nodes.csv");
    let mut body = String::new();
    body.push_str("id,label,community,community_name,lang,is_test,degree\r\n");
    for f in &g.nodes {
        let comm = match f.community {
            Some(c) => c.to_string(),
            None => String::new(), // Python None → empty field
        };
        let comm_name = f
            .community
            .and_then(|c| g.communities.get(&c).cloned())
            .unwrap_or_default();
        body.push_str(&csv_row(&[
            f.id.to_string(),
            f.name.clone(),
            comm,
            comm_name,
            f.lang.clone(),
            (f.is_test as i64).to_string(),
            deg.get(&f.id).copied().unwrap_or(0).to_string(),
        ]));
        body.push_str("\r\n");
    }
    std::fs::write(&nodes_path, body)
        .map_err(|e| format!("{} 寫入失敗：{}", nodes_path.display(), e))?;
    let links_path = out_dir.join("graph-links.csv");
    let mut body = String::new();
    body.push_str("source,target,kind\r\n");
    for e in &g.links {
        body.push_str(&csv_row(&[
            e.s.to_string(),
            e.t.to_string(),
            e.kinds.clone(),
        ]));
        body.push_str("\r\n");
    }
    std::fs::write(&links_path, body)
        .map_err(|e| format!("{} 寫入失敗：{}", links_path.display(), e))?;
    Ok((nodes_path, links_path))
}

/// Route a `code-reality graph_csv ...` invocation.
pub fn run(argv: &[&str]) -> ToolOutput {
    let Some((&_sub, toks)) = argv.split_first() else {
        return ToolOutput::fail("需提供子命令 graph_csv");
    };
    let values = match parse(&SPEC, toks) {
        Outcome::Help => {
            return ToolOutput {
                stdout: HELP.to_string(),
                stderr: String::new(),
                exit_code: 0,
            };
        }
        Outcome::Err(msg) => return ToolOutput::fail(msg),
        Outcome::Ok { values, .. } => values,
    };
    let repo = values
        .get("--repo")
        .and_then(|v| v.clone())
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let out_dir = values
        .get("--out-dir")
        .and_then(|v| v.clone())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".agent-tmp"));

    let db_path = graph_db_path(&repo);
    if !db_path.exists() {
        return ToolOutput::crash(format!(
            "graph.db 不存在：{}——先跑 `uvx code-review-graph build`",
            db_path.display()
        ));
    }
    let g = match load(&db_path, &repo) {
        Ok(g) => g,
        Err(msg) => return ToolOutput::crash(msg),
    };
    let (nodes_path, links_path) = match write_csvs(&g, &out_dir) {
        Ok(v) => v,
        Err(msg) => return ToolOutput::crash(msg),
    };
    ToolOutput {
        stdout: format!(
            "[OK] graph csv: {} nodes / {} links -> {} + {}（{}）\n",
            g.nodes.len(),
            g.links.len(),
            nodes_path
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default(),
            links_path
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default(),
            out_dir.display()
        ),
        stderr: String::new(),
        exit_code: 0,
    }
}

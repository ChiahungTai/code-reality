//! `refresh` — heal/refresh argv face (S4, ep-index-query-time-self-heal).
//!
//! `code-reality refresh --repo <repo>` is the post-commit background
//! rebuild the opt-in hook invokes: full idempotent re-produce when
//! sources moved past the slot, head-sync re-stamp only when provenance
//! lags (docs-only commits). Background face by design — heal failures
//! land on stderr with exit 0 (remediation is the query-time net,
//! SM-20), never blocking the commit that spawned it.
//!
//! `code-reality hook install|remove --repo <repo>` wires the opt-in
//! `.githooks/post-commit`. Loud refusals over existing unmanaged hooks
//! or a foreign `core.hooksPath` — never a silent overwrite; the script
//! embeds the resolved absolute bin path (GUI git clients may run hooks
//! without PATH, the lsp-bridge wrapper's known trap).

use crate::argparse::{parse, FlagSpec, Kind, Outcome, ToolSpec};
use crate::build::{ensure_fresh, producer_roots, HealOutcome};
use crate::common::resolve_bin;
use crate::engine::{default_index_path, evaluate_staleness, resolve_repo, stamp_meta_core};
use crate::ToolOutput;
use std::path::{Path, PathBuf};

const HELP: &str = "usage: code-reality refresh --repo <repo>   (background index refresh)
       code-reality hook install --repo <repo>  (opt-in post-commit wiring)
       code-reality hook remove --repo <repo>
";

const HOOK_MARKER: &str = "# code-reality post-commit refresh (opt-in)";

const REFRESH_SPEC: ToolSpec = ToolSpec {
    flags: &[FlagSpec {
        long: "--repo",
        short: None,
        kind: Kind::Value { metavar: "REPO" },
    }],
    positionals: &[],
};

const HOOK_SPEC: ToolSpec = ToolSpec {
    flags: &[FlagSpec {
        long: "--repo",
        short: None,
        kind: Kind::Value { metavar: "REPO" },
    }],
    positionals: &["install|remove"],
};

pub fn run(argv: &[&str]) -> ToolOutput {
    match argv.first() {
        Some(&"refresh") => refresh_run(&argv[1..]),
        Some(&"hook") => hook_run(&argv[1..]),
        None => ToolOutput::fail("需提供子命令 refresh 或 hook"),
        _ => ToolOutput::fail(format!("未知子命令：{}", argv[0])),
    }
}

fn help_output() -> ToolOutput {
    ToolOutput {
        stdout: HELP.to_string(),
        stderr: String::new(),
        exit_code: 0,
    }
}

fn refresh_run(toks: &[&str]) -> ToolOutput {
    let values = match parse(&REFRESH_SPEC, toks) {
        Outcome::Help => return help_output(),
        Outcome::Err(m) => return ToolOutput::fail(m),
        Outcome::Ok { values, .. } => values,
    };
    let Some(repo_s) = values.get("--repo").and_then(|v| v.clone()) else {
        return ToolOutput::fail("the following arguments are required: --repo");
    };
    let repo = resolve_repo(Path::new(&repo_s));
    let slot = match default_index_path(&repo) {
        Ok(s) => s,
        Err(e) => {
            return ToolOutput {
                stdout: String::new(),
                stderr: crate::msg_line("WARN", &e),
                exit_code: 0,
            }
        }
    };
    let mut stderr = String::new();
    // Snapshot first (Fresh is a unit variant — the head-sync decision
    // needs the drift bit the outcome does not carry).
    let snap = match evaluate_staleness(&repo, &slot) {
        Ok(s) => s,
        Err(e) => {
            stderr.push_str(&format!(
                "[WARN] 索引過期檢查失敗（{e}）——背景 refresh 結束\n"
            ));
            return ToolOutput {
                stdout: String::new(),
                stderr,
                exit_code: 0,
            };
        }
    };
    match ensure_fresh(&repo, &producer_roots()) {
        Ok(HealOutcome::Fresh) => {
            if snap.head_drift == Some(true) && slot.exists() {
                // head-sync only: sources are current, provenance lags —
                // re-stamp instead of paying a full re-produce
                match stamp_meta_core(&repo, &slot, &producer_roots(), None) {
                    Ok(_) => stderr.push_str("[OK] refresh：索引新鮮，meta head 已同步\n"),
                    Err(e) => {
                        stderr.push_str(&format!("[WARN] refresh：head-sync stamp 失敗（{e}）\n"))
                    }
                }
            }
        }
        Ok(HealOutcome::Healed {
            secs, nodes, notes, ..
        }) => {
            stderr.push_str(&format!(
                "[OK] refresh：索引已重產（{secs:.1}s，{nodes} nodes）\n"
            ));
            for n in notes {
                stderr.push_str(&n);
            }
        }
        Ok(HealOutcome::HealedByPeer { .. }) => {
            stderr.push_str("[OK] refresh：同 slot 並發 heal，等待後重用\n");
        }
        Ok(HealOutcome::ServeStale(lines)) => {
            for l in lines {
                stderr.push_str(&l);
            }
            stderr.push_str("[WARN] refresh：以現存索引收尾\n");
        }
        Err(e) => stderr.push_str(&format!(
            "[WARN] 索引過期檢查失敗（{e}）——背景 refresh 結束\n"
        )),
    }
    ToolOutput {
        stdout: String::new(),
        stderr,
        exit_code: 0,
    }
}

fn hook_run(toks: &[&str]) -> ToolOutput {
    let (values, positionals) = match parse(&HOOK_SPEC, toks) {
        Outcome::Help => return help_output(),
        Outcome::Err(m) => return ToolOutput::fail(m),
        Outcome::Ok {
            values,
            positionals,
        } => (values, positionals),
    };
    let Some(repo_s) = values.get("--repo").and_then(|v| v.clone()) else {
        return ToolOutput::fail("the following arguments are required: --repo");
    };
    let repo = resolve_repo(Path::new(&repo_s));
    match positionals.first().map(String::as_str) {
        Some("install") => hook_install(&repo, &producer_roots()),
        Some("remove") => hook_remove(&repo),
        other => ToolOutput::fail(format!(
            "hook 位置參數需為 install 或 remove（得到 {}）",
            other.unwrap_or("無")
        )),
    }
}

fn git_config_get(repo: &Path, key: &str) -> Option<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["config", "--get", key])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn git_config_set(repo: &Path, key: &str, value: &str) -> Result<(), String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["config", key, value])
        .output()
        .map_err(|e| format!("git config {key} 執行失敗：{e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git config {key} {value} 失敗：{}",
            String::from_utf8_lossy(&out.stderr).trim_end()
        ));
    }
    Ok(())
}

fn git_config_unset(repo: &Path, key: &str) -> Result<(), String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["config", "--unset", key])
        .output()
        .map_err(|e| format!("git config --unset {key} 執行失敗：{e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git config --unset {key} 失敗：{}",
            String::from_utf8_lossy(&out.stderr).trim_end()
        ));
    }
    Ok(())
}

/// Install the opt-in post-commit hook. Public with injectable roots so
/// tests resolve the bin from synthetic dirs (the script embeds the
/// resolved absolute path — GUI git clients may run hooks without PATH).
pub fn hook_install(repo: &Path, roots: &[PathBuf]) -> ToolOutput {
    let hooks_dir = repo.join(".githooks");
    let hook_path = hooks_dir.join("post-commit");
    if let Ok(existing) = std::fs::read_to_string(&hook_path) {
        if !existing.contains(HOOK_MARKER) {
            let head: Vec<&str> = existing.lines().take(3).collect();
            return ToolOutput::fail(format!(
                ".githooks/post-commit 已存在且非 code-reality 管理（前 3 行：{}）——不覆蓋；請手動併入 refresh 行（nohup <code-reality> refresh --repo … &）",
                head.join(" ⏎ ")
            ));
        }
        return ToolOutput {
            stdout: crate::msg_line(
                "OK",
                &format!("hook 已安裝（冪等）：{}", hook_path.display()),
            ),
            stderr: String::new(),
            exit_code: 0,
        };
    }
    if let Some(cur) = git_config_get(repo, "core.hooksPath") {
        if cur != ".githooks" {
            return ToolOutput::fail(format!(
                "core.hooksPath 已設為 {cur}（可能是 husky 等既有配置）——不覆寫；確認後手動：git config core.hooksPath .githooks"
            ));
        }
    }
    // Flipping core.hooksPath silently disables .git/hooks/* — refuse
    // over active local hooks instead (post-build finding F10).
    let active_local_hooks: Vec<String> = std::fs::read_dir(repo.join(".git/hooks"))
        .map(|rd| {
            rd.flatten()
                .filter(|e| {
                    let n = e.file_name().to_string_lossy().into_owned();
                    !n.ends_with(".sample")
                })
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    if !active_local_hooks.is_empty() {
        return ToolOutput::fail(format!(
            ".git/hooks/ 已有作用中 hooks（{}）——設定 core.hooksPath 會停用它們；如確定，先手動遷移或移除",
            active_local_hooks.join("、")
        ));
    }
    let bin = match resolve_bin("code-reality", roots, "安裝：uv tool install code-reality") {
        Ok(b) => b,
        Err(e) => return ToolOutput::fail(e),
    };
    if let Err(e) = std::fs::create_dir_all(&hooks_dir) {
        return ToolOutput::fail(format!("建立 {} 失敗：{e}", hooks_dir.display()));
    }
    let script = format!(
        "#!/bin/sh\n{HOOK_MARKER}\n# installed by `code-reality hook install`; remove with `code-reality hook remove`\n# prefer `uv tool install` over uvx — this script pins the absolute bin path resolved at install time\nmkdir -p .code-reality\nnohup '{}' refresh --repo \"$(git rev-parse --show-toplevel)\" >> .code-reality/refresh.log 2>&1 &\n",
        bin.display()
    );
    if let Err(e) = std::fs::write(&hook_path, &script) {
        return ToolOutput::fail(format!("寫入 {} 失敗：{e}", hook_path.display()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755));
    }
    if git_config_get(repo, "core.hooksPath").as_deref() != Some(".githooks") {
        if let Err(e) = git_config_set(repo, "core.hooksPath", ".githooks") {
            return ToolOutput::fail(e);
        }
    }
    ToolOutput {
        stdout: format!(
            "[OK] hook installed：{}\n  注意：設定 core.hooksPath 會停用 .git/hooks/* 既有 hooks\n  還原：code-reality hook remove --repo {}\n",
            hook_path.display(),
            repo.display()
        ),
        stderr: String::new(),
        exit_code: 0,
    }
}

/// Reverse of [`hook_install`]: removes our hook file (never an unmanaged
/// one) and unsets `core.hooksPath` only when the value is still ours.
pub fn hook_remove(repo: &Path) -> ToolOutput {
    let hook_path = repo.join(".githooks/post-commit");
    let mut stdout = String::new();
    match std::fs::read_to_string(&hook_path) {
        Ok(existing) if existing.contains(HOOK_MARKER) => match std::fs::remove_file(&hook_path) {
            Ok(()) => stdout.push_str(&format!("[OK] hook removed：{}\n", hook_path.display())),
            Err(e) => stdout.push_str(&format!("[WARN] 移除失敗（{e}）\n")),
        },
        Ok(_) => stdout.push_str("[WARN] post-commit 非 code-reality 管理——未刪除\n"),
        Err(_) => stdout.push_str("[WARN] 找不到 .githooks/post-commit\n"),
    }
    if git_config_get(repo, "core.hooksPath").as_deref() == Some(".githooks") {
        match git_config_unset(repo, "core.hooksPath") {
            Ok(()) => {
                stdout.push_str("[OK] 已 unset core.hooksPath（若安裝前有其他值，請自行還原）\n")
            }
            Err(e) => stdout.push_str(&format!("[WARN] {e}\n")),
        }
    }
    ToolOutput {
        stdout,
        stderr: String::new(),
        exit_code: 0,
    }
}

//! S1 lib-skeleton contract: ToolOutput shape + `[FAIL]` line convention.

use code_reality::ToolOutput;

#[test]
fn fail_output_is_stderr_fail_line_with_exit_2() {
    let out = ToolOutput::fail("something broke");
    assert_eq!(out.stdout, "");
    assert_eq!(out.stderr, "[FAIL] something broke\n");
    assert_eq!(out.exit_code, 2);
}

#[test]
fn msg_line_appends_trailing_newline() {
    assert_eq!(code_reality::msg_line("OK", "x"), "[OK] x\n");
}

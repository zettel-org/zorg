use std::process::Command;

#[test]
fn zorg_help_works() {
    let output = Command::new(env!("CARGO_BIN_EXE_zorg"))
        .arg("--help")
        .output()
        .expect("run zorg --help");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("help should be utf8");
    assert!(stdout.contains("Usage: zorg"));
}

#[test]
fn zorg_parse_emits_pretty_model_json() {
    let fixture = format!(
        "{}/../../fixtures/corpus/minimal.z",
        env!("CARGO_MANIFEST_DIR")
    );
    let output = Command::new(env!("CARGO_BIN_EXE_zorg"))
        .args(["parse", fixture.as_str()])
        .output()
        .expect("run zorg parse");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("parse output should be utf8");
    assert!(stdout.contains("\"id\": \"minimal\""));
    assert!(stdout.contains("\"tag\": \"z/ref\""));
    assert!(stdout.contains("\"key\": \"area\""));
    assert!(stdout.contains("\"diagnostics\": []"));
}

#[test]
fn zorg_parse_reports_unreadable_file() {
    let fixture = format!(
        "{}/../../fixtures/corpus/missing.z",
        env!("CARGO_MANIFEST_DIR")
    );
    let output = Command::new(env!("CARGO_BIN_EXE_zorg"))
        .args(["parse", fixture.as_str()])
        .output()
        .expect("run zorg parse");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("error output should be utf8");
    assert!(stderr.contains("failed to read"));
}

#[test]
fn zorg_check_reports_strict_semantic_errors() {
    let fixture = format!(
        "{}/../../fixtures/corpus/legacy_invalid.z",
        env!("CARGO_MANIFEST_DIR")
    );
    let output = Command::new(env!("CARGO_BIN_EXE_zorg"))
        .args(["check", fixture.as_str()])
        .output()
        .expect("run zorg check");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("check output should be utf8");
    assert!(stderr.contains("legacy.unsupported"));
    assert!(stderr.contains("ID::"));
}

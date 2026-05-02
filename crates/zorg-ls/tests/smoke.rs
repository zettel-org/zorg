use std::process::Command;

#[test]
fn zorg_ls_version_works() {
    let output = Command::new(env!("CARGO_BIN_EXE_zorg-ls"))
        .arg("--version")
        .output()
        .expect("run zorg-ls --version");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("version should be utf8");
    assert!(stdout.starts_with("zorg-ls "));
}

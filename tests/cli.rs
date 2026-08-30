use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn fixture(name: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is after the Unix epoch")
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("repolint-{name}-{}-{suffix}", std::process::id()));
    fs::create_dir(&path).expect("fixture directory can be created");
    path
}

fn write(path: &std::path::Path, contents: &str) {
    fs::write(path, contents).expect("fixture file can be written");
}

#[test]
fn map_write_then_check_is_clean() {
    let root = fixture("map");
    fs::create_dir(root.join("src")).expect("fixture source directory can be created");
    write(
        &root.join("README.md"),
        "# Fixture\n\nA repository used to test map generation.\n\n## Run\n",
    );
    write(
        &root.join("repolint.toml"),
        "[repo]\ntier = \"workspace\"\nkind = \"app\"\nvisibility = \"private\"\nresearch = false\n\n[map.dirs]\nsrc = \"Source code\"\n",
    );

    let map = Command::new(env!("CARGO_BIN_EXE_repolint"))
        .current_dir(&root)
        .args(["map", "--write"])
        .output()
        .expect("map command starts");
    assert!(
        map.status.success(),
        "map failed: {}",
        String::from_utf8_lossy(&map.stderr)
    );

    let check = Command::new(env!("CARGO_BIN_EXE_repolint"))
        .current_dir(&root)
        .arg("check")
        .output()
        .expect("check command starts");
    assert!(
        check.status.success(),
        "check failed: {}",
        String::from_utf8_lossy(&check.stdout)
    );
    assert!(String::from_utf8_lossy(&check.stdout).contains("clean"));
    fs::remove_dir_all(root).expect("fixture cleanup");
}

#[test]
fn workspace_without_map_fails() {
    let root = fixture("missing-map");
    write(
        &root.join("README.md"),
        "# Fixture\n\nA workspace fixture.\n",
    );
    write(
        &root.join("repolint.toml"),
        "[repo]\ntier = \"workspace\"\nkind = \"app\"\nvisibility = \"private\"\nresearch = false\n",
    );

    let check = Command::new(env!("CARGO_BIN_EXE_repolint"))
        .current_dir(&root)
        .args(["check", "--format", "json"])
        .output()
        .expect("check command starts");
    assert_eq!(check.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&check.stdout).contains("README.md"));
    fs::remove_dir_all(root).expect("fixture cleanup");
}

#[test]
fn unadopted_repository_is_warn_only() {
    let root = fixture("unadopted");
    let check = Command::new(env!("CARGO_BIN_EXE_repolint"))
        .current_dir(&root)
        .arg("check")
        .output()
        .expect("check command starts");
    assert!(check.status.success());
    assert!(String::from_utf8_lossy(&check.stdout).contains("not adopted"));
    fs::remove_dir_all(root).expect("fixture cleanup");
}

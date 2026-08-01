use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static FIXTURE_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn current_repository() -> Self {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "dornglut-github-validation-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();

        let output = Command::new("git")
            .args(["ls-files", "-z"])
            .current_dir(source)
            .output()
            .expect("git must be available for validation fixtures");
        assert!(output.status.success(), "git ls-files failed: {output:?}");
        let mut paths = output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
            .map(|path| PathBuf::from(std::ffi::OsString::from_vec(path.to_vec())))
            .collect::<Vec<_>>();
        paths.sort();
        for relative in paths {
            let source_path = source.join(&relative);
            if !source_path.is_file() {
                continue;
            }
            let destination = root.join(&relative);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::copy(&source_path, destination).unwrap();
        }
        Self { root }
    }

    fn write(&self, relative: &str, bytes: &[u8]) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn remove(&self, relative: &str) {
        fs::remove_file(self.root.join(relative)).unwrap();
    }

    fn failure(&self) -> String {
        xtask::validate_repository(&self.root)
            .unwrap_err()
            .to_string()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if self.root.starts_with(std::env::temp_dir()) {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }
}

#[test]
fn valid_current_repository_passes() {
    let fixture = Fixture::current_repository();
    assert!(xtask::validate_repository(&fixture.root).is_ok());
}

#[test]
fn missing_required_file_is_rejected() {
    let fixture = Fixture::current_repository();
    fixture.remove("README.md");
    assert_contains(fixture.failure(), "README.md: required file is missing");
}

#[test]
fn empty_required_file_is_rejected() {
    let fixture = Fixture::current_repository();
    fixture.write("README.md", b"");
    assert_contains(fixture.failure(), "README.md: required file is empty");
}

#[test]
fn retired_issue_template_is_rejected() {
    let fixture = Fixture::current_repository();
    fixture.write(".github/ISSUE_TEMPLATE/bug.yml", b"name: retired\n");
    assert_contains(
        fixture.failure(),
        ".github/ISSUE_TEMPLATE/bug.yml: retired issue form must not exist",
    );
}

#[test]
fn invalid_utf8_is_rejected() {
    let fixture = Fixture::current_repository();
    fixture.write("notes/invalid.MD", &[0xff]);
    assert_contains(fixture.failure(), "notes/invalid.MD: is not valid UTF-8");
}

#[test]
fn nul_byte_is_rejected() {
    let fixture = Fixture::current_repository();
    fixture.write("notes/nul.md", b"hello\0world\n");
    assert_contains(fixture.failure(), "notes/nul.md: contains a NUL byte");
}

#[test]
fn missing_final_newline_is_rejected_for_uppercase_extension() {
    let fixture = Fixture::current_repository();
    fixture.write("notes/UPPER.MD", b"text");
    assert_contains(fixture.failure(), "notes/UPPER.MD: must end with a newline");
}

#[test]
fn trailing_whitespace_and_tabs_are_rejected() {
    let fixture = Fixture::current_repository();
    fixture.write("notes/spacing.YmL", b"key: value \n\tother: value\n");
    let failure = fixture.failure();
    assert_contains(&failure, "notes/spacing.YmL:1: trailing whitespace");
    assert_contains(&failure, "notes/spacing.YmL:2: tab character");
}

#[test]
fn broken_relative_link_is_rejected() {
    let fixture = Fixture::current_repository();
    fixture.write("notes/link.md", b"[missing](missing.md)\n");
    assert_contains(
        fixture.failure(),
        "notes/link.md: broken relative link: missing.md",
    );
}

#[test]
fn malformed_link_does_not_hide_a_later_link_candidate() {
    let fixture = Fixture::current_repository();
    fixture.write(
        "notes/link.md",
        b"[malformed] text\n[missing](missing.md)\n",
    );
    assert_contains(
        fixture.failure(),
        "notes/link.md: broken relative link: missing.md",
    );
}

#[test]
fn query_and_fragment_do_not_break_a_valid_relative_link() {
    let fixture = Fixture::current_repository();
    fixture.write("notes/target.md", b"# Target\n");
    fixture.write("notes/link.md", b"[target](target.md?view=full#target)\n");
    assert!(xtask::validate_repository(&fixture.root).is_ok());
}

#[test]
fn repository_escaping_link_is_rejected() {
    let fixture = Fixture::current_repository();
    fixture.write("notes/link.md", b"[escape](../../outside.md)\n");
    assert_contains(
        fixture.failure(),
        "notes/link.md: link escapes repository: ../../outside.md",
    );
}

#[test]
fn historical_owner_token_is_rejected_on_active_surface() {
    let fixture = Fixture::current_repository();
    fixture.write(
        "profile/README.md",
        b"[old](https://github.com/crystonix/example)\n",
    );
    assert_contains(
        fixture.failure(),
        "profile/README.md: contains historical owner token \"github.com/crystonix/\"",
    );
}

#[test]
fn issue_template_inventory_drift_is_rejected() {
    let fixture = Fixture::current_repository();
    fixture.write(
        ".github/ISSUE_TEMPLATE/unexpected.yml",
        b"name: Unexpected\n",
    );
    assert_contains(fixture.failure(), "issue-template inventory mismatch");
}

#[test]
fn workflow_template_inventory_drift_is_rejected() {
    let fixture = Fixture::current_repository();
    fixture.write("workflow-templates/unexpected.yml", b"name: Unexpected\n");
    assert_contains(fixture.failure(), "workflow-template inventory mismatch");
}

#[test]
fn unauthorized_active_workflow_is_rejected() {
    let fixture = Fixture::current_repository();
    fixture.write(".github/workflows/unexpected.yml", b"name: Unexpected\n");
    assert_contains(fixture.failure(), "active workflow inventory drift");
}

#[test]
fn mutable_reusable_workflow_reference_is_rejected() {
    let fixture = Fixture::current_repository();
    replace(
        &fixture.root.join(".github/workflows/validate.yml"),
        "@624cb41adeed21a6461eb838bc7330bd0a5079fd",
        "@main",
    );
    assert_contains(
        fixture.failure(),
        ".github/workflows/validate.yml: immutable revision drift",
    );
}

#[test]
fn incorrect_branch_target_is_rejected() {
    let fixture = Fixture::current_repository();
    replace(
        &fixture.root.join(".github/workflows/validate.yml"),
        "- main",
        "- trunk",
    );
    assert_contains(
        fixture.failure(),
        ".github/workflows/validate.yml: workflow branch drift",
    );
}

#[test]
fn incorrect_workflow_permission_is_rejected() {
    let fixture = Fixture::current_repository();
    replace(
        &fixture.root.join(".github/workflows/validate.yml"),
        "contents: read",
        "contents: write",
    );
    assert_contains(
        fixture.failure(),
        ".github/workflows/validate.yml: workflow permission drift",
    );
}

#[test]
fn extra_active_workflow_field_is_rejected() {
    let fixture = Fixture::current_repository();
    fixture.write(
        ".github/workflows/validate.yml",
        format!(
            "{}timeout-minutes: 5\n",
            fs::read_to_string(fixture.root.join(".github/workflows/validate.yml")).unwrap()
        )
        .as_bytes(),
    );
    assert_contains(
        fixture.failure(),
        ".github/workflows/validate.yml: unexpected workflow field or ordering drift",
    );
}

#[test]
fn malformed_workflow_properties_are_rejected() {
    let fixture = Fixture::current_repository();
    fixture.write("workflow-templates/rust-validation.properties.json", b"{\n");
    assert_contains(
        fixture.failure(),
        "workflow-templates/rust-validation.properties.json: invalid JSON",
    );
}

#[test]
fn missing_workflow_property_pair_is_rejected() {
    let fixture = Fixture::current_repository();
    fixture.remove("workflow-templates/rust-validation.yml");
    assert_contains(
        fixture.failure(),
        "workflow-templates/rust-validation.properties.json: matching workflow template is missing",
    );
}

#[test]
fn reintroduced_python_validator_is_rejected() {
    let fixture = Fixture::current_repository();
    fixture.write("scripts/validate.py", b"#!/usr/bin/env python3\n");
    assert_contains(
        fixture.failure(),
        "scripts/validate.py: retired Python validator must not exist",
    );
}

fn replace(path: &Path, from: &str, to: &str) {
    let text = fs::read_to_string(path).unwrap();
    assert!(text.contains(from), "replacement source must be present");
    fs::write(path, text.replacen(from, to, 1)).unwrap();
}

fn assert_contains(value: impl AsRef<str>, expected: &str) {
    assert!(
        value.as_ref().contains(expected),
        "expected {expected:?} in:\n{}",
        value.as_ref()
    );
}

trait OsStringFromVec {
    fn from_vec(value: Vec<u8>) -> Self;
}

#[cfg(unix)]
impl OsStringFromVec for std::ffi::OsString {
    fn from_vec(value: Vec<u8>) -> Self {
        use std::os::unix::ffi::OsStringExt;
        <Self as OsStringExt>::from_vec(value)
    }
}

#[cfg(not(unix))]
impl OsStringFromVec for std::ffi::OsString {
    fn from_vec(value: Vec<u8>) -> Self {
        String::from_utf8(value)
            .expect("Git path must use UTF-8 on this platform")
            .into()
    }
}

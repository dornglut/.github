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

        copy_tracked_files(source, &root);
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

    #[cfg(unix)]
    fn symlink(&self, relative: &str, target: &Path) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(target, path).unwrap();
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
fn python_text_suffixes_remain_covered() {
    struct Case {
        path: &'static str,
        bytes: &'static [u8],
        expected: &'static [&'static str],
    }

    for case in [
        Case {
            path: "notes/invalid.py",
            bytes: &[0xff],
            expected: &["notes/invalid.py: is not valid UTF-8"],
        },
        Case {
            path: "notes/nul.PY",
            bytes: b"print('x')\0\n",
            expected: &["notes/nul.PY: contains a NUL byte"],
        },
        Case {
            path: "notes/newline.Py",
            bytes: b"print('x')",
            expected: &["notes/newline.Py: must end with a newline"],
        },
        Case {
            path: "notes/spacing.PY",
            bytes: b"print('x') \n\tprint('y')\n",
            expected: &[
                "notes/spacing.PY:1: trailing whitespace",
                "notes/spacing.PY:2: tab character",
            ],
        },
    ] {
        let fixture = Fixture::current_repository();
        fixture.write(case.path, case.bytes);
        let failure = fixture.failure();
        for expected in case.expected {
            assert_contains(&failure, expected);
        }
    }
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
fn markdown_link_parity_cases_match_the_accepted_python_behavior() {
    struct Case {
        name: &'static str,
        markdown: &'static str,
        files: &'static [&'static str],
        expected_failure: Option<&'static str>,
    }

    for case in [
        Case {
            name: "empty label",
            markdown: "[](missing.md)\n",
            files: &[],
            expected_failure: Some("notes/link.md: broken relative link: missing.md"),
        },
        Case {
            name: "empty image alt text",
            markdown: "![](missing.png)\n",
            files: &[],
            expected_failure: Some("notes/link.md: broken relative link: missing.png"),
        },
        Case {
            name: "encoded fragment character",
            markdown: "[file](file%23name.md)\n",
            files: &["notes/file#name.md"],
            expected_failure: None,
        },
        Case {
            name: "encoded query character",
            markdown: "[file](file%3Fname.md)\n",
            files: &["notes/file?name.md"],
            expected_failure: None,
        },
        Case {
            name: "literal fragment",
            markdown: "[file](file.md#section)\n",
            files: &["notes/file.md"],
            expected_failure: None,
        },
        Case {
            name: "literal query",
            markdown: "[file](file.md?view=1)\n",
            files: &["notes/file.md"],
            expected_failure: None,
        },
        Case {
            name: "encoded traversal",
            markdown: "[escape](%2E%2E/%2E%2E/outside.md)\n",
            files: &[],
            expected_failure: Some(
                "notes/link.md: link escapes repository: %2E%2E/%2E%2E/outside.md",
            ),
        },
        Case {
            name: "external links",
            markdown: "[web](https://example.com) [mail](mailto:maintainer@example.com)\n",
            files: &[],
            expected_failure: None,
        },
        Case {
            name: "pure fragment",
            markdown: "[section](#section)\n",
            files: &[],
            expected_failure: None,
        },
        Case {
            name: "multiple candidates",
            markdown: "[present](file.md) [missing](missing.md)\n",
            files: &["notes/file.md"],
            expected_failure: Some("notes/link.md: broken relative link: missing.md"),
        },
        Case {
            name: "malformed candidate followed by a valid candidate",
            markdown: "[malformed] prose [missing](missing.md)\n",
            files: &[],
            expected_failure: Some("notes/link.md: broken relative link: missing.md"),
        },
    ] {
        let fixture = Fixture::current_repository();
        for path in case.files {
            fixture.write(path, b"fixture\n");
        }
        fixture.write("notes/link.md", case.markdown.as_bytes());
        match case.expected_failure {
            Some(expected) => assert_contains(fixture.failure(), expected),
            None => assert!(
                xtask::validate_repository(&fixture.root).is_ok(),
                "{} should pass",
                case.name
            ),
        }
    }
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

#[cfg(unix)]
#[test]
fn symlinked_repository_surfaces_and_markdown_targets_are_rejected() {
    let fixture = Fixture::current_repository();
    fixture.write("notes/internal.md", b"internal\n");
    fixture.symlink("notes/alias.md", Path::new("internal.md"));
    fixture.write("notes/link.md", b"[alias](alias.md)\n");
    assert_contains(
        fixture.failure(),
        "notes/alias.md: symbolic links are not permitted",
    );
}

#[cfg(unix)]
#[test]
fn symlinked_markdown_parent_directory_is_rejected() {
    let fixture = Fixture::current_repository();
    fixture.write("notes/targets/target.md", b"target\n");
    fixture.symlink("notes/linked", Path::new("targets"));
    fixture.write("notes/link.md", b"[target](linked/target.md)\n");
    assert_contains(
        fixture.failure(),
        "notes/linked/target.md: symbolic links are not permitted",
    );
}

#[cfg(unix)]
#[test]
fn external_symlink_markdown_target_reports_a_repository_escape_without_reading_it() {
    let fixture = Fixture::current_repository();
    let outside = std::env::temp_dir().join(format!(
        "dornglut-external-symlink-target-{}",
        FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&outside, b"outside\n").unwrap();
    fixture.symlink("notes/external.md", &outside);
    fixture.write("notes/link.md", b"[external](external.md)\n");
    assert_contains(
        fixture.failure(),
        "notes/link.md: link escapes repository: external.md",
    );
    fs::remove_file(outside).unwrap();
}

#[cfg(unix)]
#[test]
fn broken_symlink_markdown_target_is_rejected() {
    let fixture = Fixture::current_repository();
    fixture.symlink("notes/broken.md", Path::new("missing.md"));
    fixture.write("notes/link.md", b"[broken](broken.md)\n");
    assert_contains(
        fixture.failure(),
        "notes/broken.md: symbolic links are not permitted",
    );
}

#[cfg(unix)]
#[test]
fn required_symlinked_file_is_rejected() {
    let fixture = Fixture::current_repository();
    fixture.write("notes/readme.md", b"replacement\n");
    fixture.remove("README.md");
    fixture.symlink("README.md", Path::new("notes/readme.md"));
    assert_contains(
        fixture.failure(),
        "README.md: symbolic links are not permitted",
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
fn malformed_workflows_are_rejected_without_panicking() {
    struct Case {
        name: &'static str,
        workflow: &'static [u8],
        expected: &'static str,
    }

    for case in [
        Case {
            name: "empty",
            workflow: b"",
            expected: "workflow identity drift",
        },
        Case {
            name: "whitespace only",
            workflow: b" \n\t\n",
            expected: "workflow identity drift",
        },
        Case {
            name: "name only",
            workflow: b"name: Validate\n",
            expected: "workflow trigger drift",
        },
        Case {
            name: "truncated trigger",
            workflow: b"name: Validate\non:\n  pull_request:\n  push:\n",
            expected: "workflow branch drift",
        },
        Case {
            name: "truncated permissions",
            workflow: b"name: Validate\non:\n  pull_request:\n  push:\n    branches:\n      - main\npermissions:\n",
            expected: "workflow permission drift",
        },
        Case {
            name: "truncated job",
            workflow: b"name: Validate\non:\n  pull_request:\n  push:\n    branches:\n      - main\npermissions:\n  contents: read\n",
            expected: "workflow job drift",
        },
        Case {
            name: "unexpected field",
            workflow: b"name: Validate\non:\n  pull_request:\n  push:\n    branches:\n      - main\npermissions:\n  contents: read\njobs:\n  validate:\n    uses: dornglut/github-workflows/.github/workflows/reusable-rust-cargo-validate.yml@624cb41adeed21a6461eb838bc7330bd0a5079fd\ntimeout-minutes: 5\n",
            expected: "unexpected workflow field or ordering drift",
        },
    ] {
        let fixture = Fixture::current_repository();
        fixture.write(".github/workflows/validate.yml", case.workflow);
        let failure = fixture.failure();
        assert!(
            failure.contains(case.expected),
            "{} should report {:?}, got:\n{failure}",
            case.name,
            case.expected
        );
    }
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

fn copy_tracked_files(source: &Path, root: &Path) {
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
        copy_regular_file(&source.join(&relative), &root.join(relative));
    }
}

fn copy_regular_file(source: &Path, destination: &Path) {
    let metadata = fs::symlink_metadata(source).unwrap();
    assert!(
        !metadata.file_type().is_symlink(),
        "fixture source path {} is a symbolic link",
        source.display()
    );
    if !metadata.is_file() {
        return;
    }
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::copy(source, destination).unwrap();
}

#[cfg(unix)]
#[test]
fn fixture_copy_rejects_source_symlinks_without_following_them() {
    let source_root = std::env::temp_dir().join(format!(
        "dornglut-fixture-source-symlink-{}",
        FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let target = source_root.with_extension("target");
    let destination = source_root.with_extension("destination");
    fs::write(&target, b"outside fixture source\n").unwrap();
    std::os::unix::fs::symlink(&target, &source_root).unwrap();

    let failure = std::panic::catch_unwind(|| copy_regular_file(&source_root, &destination))
        .expect_err("fixture copying must reject source symlinks");
    assert_contains(panic_message(failure), "fixture source path");
    assert!(!destination.exists());

    fs::remove_file(source_root).unwrap();
    fs::remove_file(target).unwrap();
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

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_owned();
    }
    "non-string panic payload".to_owned()
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

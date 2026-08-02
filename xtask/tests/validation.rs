use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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

        copy_tracked_files(source, &root).unwrap();
        initialize_fixture_repository(&root);
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
        self.validation_result().unwrap_err().to_string()
    }

    fn validation_result(&self) -> Result<(), xtask::ValidationErrors> {
        xtask::validate_repository(&self.root)
    }
}

fn initialize_fixture_repository(root: &Path) {
    let status = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(root)
        .status()
        .unwrap();
    assert!(status.success(), "git init failed: {status}");
    let status = Command::new("git")
        .args(["add", "--all"])
        .current_dir(root)
        .status()
        .unwrap();
    assert!(status.success(), "git add failed: {status}");
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
        "notes/link.md: symbolic links are not permitted: linked/target.md",
    );
}

#[cfg(unix)]
#[test]
fn external_parent_symlink_cannot_be_erased_by_parent_traversal() {
    let fixture = Fixture::current_repository();
    let outside = std::env::temp_dir().join(format!(
        "dornglut-external-parent-symlink-{}",
        FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&outside).unwrap();
    fixture.write("notes/safe.md", b"safe\n");
    fixture.symlink("notes/linked", &outside);
    fixture.write("notes/link.md", b"[safe](linked/../safe.md)\n");

    assert_contains(
        fixture.failure(),
        "notes/link.md: link escapes repository: linked/../safe.md",
    );
    fs::remove_dir_all(outside).unwrap();
}

#[cfg(unix)]
#[test]
fn in_repository_parent_symlink_cannot_be_erased_by_parent_traversal() {
    let fixture = Fixture::current_repository();
    fixture.write("notes/targets/target.md", b"target\n");
    fixture.write("notes/safe.md", b"safe\n");
    fixture.symlink("notes/linked", Path::new("targets"));
    fixture.write("notes/link.md", b"[safe](linked/../safe.md)\n");

    assert_contains(
        fixture.failure(),
        "notes/link.md: symbolic links are not permitted: linked/../safe.md",
    );
}

#[cfg(unix)]
#[test]
fn broken_parent_symlink_has_a_source_link_diagnostic() {
    let fixture = Fixture::current_repository();
    fixture.symlink("notes/linked", Path::new("missing"));
    fixture.write("notes/link.md", b"[safe](linked/../safe.md)\n");

    assert_contains(
        fixture.failure(),
        "notes/link.md: symbolic links are not permitted: linked/../safe.md",
    );
}

#[test]
fn ordinary_parent_traversal_remains_a_valid_relative_link() {
    let fixture = Fixture::current_repository();
    fixture.write("notes/directory/placeholder.md", b"placeholder\n");
    fixture.write("notes/safe.md", b"safe\n");
    fixture.write("notes/link.md", b"[safe](directory/../safe.md)\n");

    assert!(xtask::validate_repository(&fixture.root).is_ok());
}

#[cfg(unix)]
#[test]
fn nested_external_parent_symlink_reports_a_repository_escape() {
    let fixture = Fixture::current_repository();
    let outside = std::env::temp_dir().join(format!(
        "dornglut-nested-external-parent-symlink-{}",
        FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&outside).unwrap();
    fixture.write("notes/nested/inside.md", b"inside\n");
    fixture.symlink("notes/nested/linked", &outside);
    fixture.write("notes/link.md", b"[safe](nested/linked/../inside.md)\n");

    assert_contains(
        fixture.failure(),
        "notes/link.md: link escapes repository: nested/linked/../inside.md",
    );
    fs::remove_dir_all(outside).unwrap();
}

#[test]
fn encoded_parent_traversal_is_rejected_after_decoding() {
    let fixture = Fixture::current_repository();
    fixture.write("notes/directory/placeholder.md", b"placeholder\n");
    fixture.write(
        "notes/link.md",
        b"[escape](directory/%2E%2E/%2E%2E/%2E%2E/outside.md)\n",
    );

    assert_contains(
        fixture.failure(),
        "notes/link.md: link escapes repository: directory/%2E%2E/%2E%2E/%2E%2E/outside.md",
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
        "notes/link.md: symbolic links are not permitted: broken.md",
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
        required_diagnostics: &'static [&'static str],
    }

    for case in [
        Case {
            name: "empty",
            workflow: b"",
            required_diagnostics: &["workflow identity drift", "workflow trigger drift"],
        },
        Case {
            name: "whitespace only",
            workflow: b" \n\t\n",
            required_diagnostics: &["workflow identity drift", "workflow job drift"],
        },
        Case {
            name: "name only",
            workflow: b"name: Validate\n",
            required_diagnostics: &["workflow trigger drift", "workflow permission drift"],
        },
        Case {
            name: "trigger only",
            workflow: b"name: Validate\non:\n  pull_request:\n  push:\n    branches:\n      - main\n",
            required_diagnostics: &["workflow permission drift", "workflow job drift"],
        },
        Case {
            name: "jobs only",
            workflow: b"name: Validate\non:\n  pull_request:\n  push:\n    branches:\n      - main\npermissions:\n  contents: read\njobs:\n",
            required_diagnostics: &["workflow job drift", "workflow profile or immutable revision drift"],
        },
        Case {
            name: "truncated trigger",
            workflow: b"name: Validate\non:\n  pull_request:\n  push:\n",
            required_diagnostics: &["workflow branch drift", "workflow trigger drift"],
        },
        Case {
            name: "truncated permissions",
            workflow: b"name: Validate\non:\n  pull_request:\n  push:\n    branches:\n      - main\npermissions:\n",
            required_diagnostics: &["workflow permission drift", "workflow job drift"],
        },
        Case {
            name: "truncated reusable job",
            workflow: b"name: Validate\non:\n  pull_request:\n  push:\n    branches:\n      - main\npermissions:\n  contents: read\n",
            required_diagnostics: &["workflow job drift", "workflow profile or immutable revision drift"],
        },
        Case {
            name: "unexpected top-level field",
            workflow: b"name: Validate\non:\n  pull_request:\n  push:\n    branches:\n      - main\npermissions:\n  contents: read\njobs:\n  validate:\n    uses: dornglut/github-workflows/.github/workflows/reusable-rust-cargo-validate.yml@624cb41adeed21a6461eb838bc7330bd0a5079fd\ntimeout-minutes: 5\n",
            required_diagnostics: &["unexpected workflow field or ordering drift"],
        },
        Case {
            name: "unexpected job field",
            workflow: b"name: Validate\non:\n  pull_request:\n  push:\n    branches:\n      - main\npermissions:\n  contents: read\njobs:\n  validate:\n    runs-on: ubuntu-latest\n    uses: dornglut/github-workflows/.github/workflows/reusable-rust-cargo-validate.yml@624cb41adeed21a6461eb838bc7330bd0a5079fd\n",
            required_diagnostics: &["unexpected workflow field or ordering drift", "workflow job drift"],
        },
    ] {
        let fixture = Fixture::current_repository();
        fixture.write(".github/workflows/validate.yml", case.workflow);
        assert_malformed_workflow(&fixture, case.name, case.required_diagnostics);
    }
}

fn assert_malformed_workflow(fixture: &Fixture, name: &str, required_diagnostics: &[&str]) {
    let first =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fixture.validation_result()))
            .unwrap_or_else(|payload| panic!("{name} panicked: {}", panic_message(payload)))
            .expect_err("malformed workflow must return ValidationErrors")
            .to_string();
    let second =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fixture.validation_result()))
            .unwrap_or_else(|payload| {
                panic!(
                    "{name} panicked on repeated validation: {}",
                    panic_message(payload)
                )
            })
            .expect_err("malformed workflow must remain invalid")
            .to_string();

    assert_eq!(first, second, "{name} diagnostics must be deterministic");
    let workflow_diagnostics = first
        .lines()
        .filter(|line| line.starts_with("- .github/workflows/validate.yml:"))
        .collect::<Vec<_>>();
    assert!(
        !workflow_diagnostics.is_empty(),
        "{name} must report the exact workflow path:\n{first}"
    );
    for diagnostic in required_diagnostics {
        assert!(
            workflow_diagnostics
                .iter()
                .any(|line| line.contains(diagnostic)),
            "{name} must report {diagnostic:?}:\n{first}"
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

#[derive(Debug)]
struct TrackedBlob {
    path: PathBuf,
    object_id: String,
}

fn copy_tracked_files(source: &Path, root: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .args(["ls-files", "--stage", "-z"])
        .current_dir(source)
        .output()
        .map_err(|error| format!("git must be available for validation fixtures: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git ls-files --stage failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let mut entries = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .map(parse_tracked_blob)
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    let object_ids = entries
        .iter()
        .map(|entry| entry.object_id.as_str())
        .collect::<Vec<_>>();
    let contents = read_git_blobs(source, &object_ids)?;
    for (entry, bytes) in entries.iter().zip(contents) {
        let destination = root.join(&entry.path);
        let parent = destination.parent().ok_or_else(|| {
            format!(
                "fixture destination has no parent: {}",
                destination.display()
            )
        })?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
        fs::write(&destination, bytes)
            .map_err(|error| format!("failed to write {}: {error}", destination.display()))?;
    }
    Ok(())
}

fn parse_tracked_blob(record: &[u8]) -> Result<TrackedBlob, String> {
    let Some(separator) = record.iter().position(|byte| *byte == b'\t') else {
        return Err("malformed git index record without a path separator".to_owned());
    };
    let (header, raw_path) = (&record[..separator], &record[separator + 1..]);
    let fields = header.split(|byte| *byte == b' ').collect::<Vec<_>>();
    if fields.len() != 3 || fields.iter().any(|field| field.is_empty()) {
        return Err("malformed git index record header".to_owned());
    }
    let mode = std::str::from_utf8(fields[0])
        .map_err(|_| "git index mode is not valid UTF-8".to_owned())?;
    if mode == "120000" {
        return Err("fixture source contains a symbolic link".to_owned());
    }
    if !matches!(mode, "100644" | "100755") {
        return Err(format!(
            "fixture source contains unsupported git mode {mode:?}"
        ));
    }
    let object_id = std::str::from_utf8(fields[1])
        .map_err(|_| "git object ID is not valid UTF-8".to_owned())?;
    if !(object_id.len() == 40 || object_id.len() == 64)
        || !object_id.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(format!("invalid git object ID {object_id:?}"));
    }
    if fields[2] != b"0" {
        return Err("fixture source index contains an unresolved merge stage".to_owned());
    }
    let path = path_from_git_bytes(raw_path)?;
    if path.as_os_str().is_empty()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::Prefix(_)
                    | std::path::Component::RootDir
                    | std::path::Component::ParentDir
            )
        })
    {
        return Err("fixture source contains an invalid tracked path".to_owned());
    }
    Ok(TrackedBlob {
        path,
        object_id: object_id.to_owned(),
    })
}

fn read_git_blobs(source: &Path, object_ids: &[&str]) -> Result<Vec<Vec<u8>>, String> {
    let mut child = Command::new("git")
        .args(["cat-file", "--batch"])
        .current_dir(source)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to start git cat-file: {error}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "git cat-file did not provide standard input".to_owned())?;
    for object_id in object_ids {
        stdin
            .write_all(object_id.as_bytes())
            .and_then(|()| stdin.write_all(b"\n"))
            .map_err(|error| format!("failed to request git blob {object_id}: {error}"))?;
    }
    drop(stdin);
    let output = child
        .wait_with_output()
        .map_err(|error| format!("failed to read git cat-file output: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git cat-file --batch failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let mut offset = 0;
    let mut blobs = Vec::with_capacity(object_ids.len());
    for expected_object_id in object_ids {
        let header_end = output.stdout[offset..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|index| offset + index)
            .ok_or_else(|| "truncated git cat-file header".to_owned())?;
        let header = std::str::from_utf8(&output.stdout[offset..header_end])
            .map_err(|_| "git cat-file header is not valid UTF-8".to_owned())?;
        let fields = header.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 3 || fields[0] != *expected_object_id || fields[1] != "blob" {
            return Err(format!("unexpected git cat-file response {header:?}"));
        }
        let size = fields[2]
            .parse::<usize>()
            .map_err(|_| format!("invalid git blob size in {header:?}"))?;
        let contents_start = header_end + 1;
        let contents_end = contents_start
            .checked_add(size)
            .ok_or_else(|| "git blob size overflow".to_owned())?;
        if output.stdout.get(contents_end) != Some(&b'\n') {
            return Err("truncated git blob contents".to_owned());
        }
        blobs.push(output.stdout[contents_start..contents_end].to_vec());
        offset = contents_end + 1;
    }
    if offset != output.stdout.len() {
        return Err("unexpected trailing git cat-file output".to_owned());
    }
    Ok(blobs)
}

#[cfg(unix)]
fn path_from_git_bytes(bytes: &[u8]) -> Result<PathBuf, String> {
    use std::os::unix::ffi::OsStringExt;

    Ok(PathBuf::from(OsString::from_vec(bytes.to_vec())))
}

#[cfg(not(unix))]
fn path_from_git_bytes(bytes: &[u8]) -> Result<PathBuf, String> {
    let value = std::str::from_utf8(bytes)
        .map_err(|_| "Git path is not valid UTF-8 on this platform".to_owned())?;
    Ok(PathBuf::from(value))
}

fn initialize_temporary_git_repository(root: &Path) {
    fs::create_dir_all(root).unwrap();
    initialize_fixture_repository(root);
}

#[test]
fn fixture_copy_materializes_indexed_git_blobs_instead_of_worktree_paths() {
    let source = temporary_path("dornglut-fixture-blob-source");
    let destination = temporary_path("dornglut-fixture-blob-destination");
    initialize_temporary_git_repository(&source);
    fs::write(source.join("tracked.md"), b"indexed blob\n").unwrap();
    let status = Command::new("git")
        .args(["add", "tracked.md"])
        .current_dir(&source)
        .status()
        .unwrap();
    assert!(status.success(), "git add failed: {status}");
    fs::write(source.join("tracked.md"), b"worktree mutation\n").unwrap();

    copy_tracked_files(&source, &destination).unwrap();
    assert_eq!(
        fs::read(destination.join("tracked.md")).unwrap(),
        b"indexed blob\n"
    );

    fs::remove_dir_all(source).unwrap();
    fs::remove_dir_all(destination).unwrap();
}

#[cfg(unix)]
#[test]
fn fixture_copy_rejects_indexed_source_symlinks_without_following_them() {
    let source = temporary_path("dornglut-fixture-symlink-source");
    let destination = temporary_path("dornglut-fixture-symlink-destination");
    initialize_temporary_git_repository(&source);
    fs::write(source.join("target.md"), b"target\n").unwrap();
    std::os::unix::fs::symlink("target.md", source.join("alias.md")).unwrap();
    let status = Command::new("git")
        .args(["add", "target.md", "alias.md"])
        .current_dir(&source)
        .status()
        .unwrap();
    assert!(status.success(), "git add failed: {status}");

    assert_contains(
        copy_tracked_files(&source, &destination).unwrap_err(),
        "fixture source contains a symbolic link",
    );
    assert!(!destination.exists());

    fs::remove_dir_all(source).unwrap();
}

fn temporary_path(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))
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

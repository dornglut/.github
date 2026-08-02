use serde_json::Value;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fmt::{Display, Formatter};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitStatus, Output};

const TEXT_SUFFIXES: &[&str] = &["json", "md", "py", "toml", "txt", "yaml", "yml"];
const REUSABLE_REVISION: &str = "624cb41adeed21a6461eb838bc7330bd0a5079fd";
const REUSABLE_WORKFLOW_OWNER: &str = "dornglut/github-workflows/.github/workflows";
const RETIRED_VALIDATOR_PATH: &str = "scripts/validate.py";

const REQUIRED_ISSUE_TEMPLATES: &[&str] = &[
    ".github/ISSUE_TEMPLATE/config.yml",
    ".github/ISSUE_TEMPLATE/defect.yml",
    ".github/ISSUE_TEMPLATE/proposal.yml",
];
const REQUIRED_WORKFLOW_TEMPLATES: &[&str] = &[
    "workflow-templates/documentation-validation.properties.json",
    "workflow-templates/documentation-validation.yml",
    "workflow-templates/rust-validation.properties.json",
    "workflow-templates/rust-validation.yml",
];
const REQUIRED_ACTIVE_WORKFLOWS: &[&str] = &[".github/workflows/validate.yml"];
const FORBIDDEN_PATHS: &[&str] = &[
    ".github/ISSUE_TEMPLATE/architecture.yml",
    ".github/ISSUE_TEMPLATE/bug.yml",
    ".github/ISSUE_TEMPLATE/feature.yml",
    RETIRED_VALIDATOR_PATH,
];
const NAMESPACE_EXACT_PATHS: &[&str] = &[
    "AGENTS.md",
    "CODE_OF_CONDUCT.md",
    "CONTRIBUTING.md",
    "GOVERNANCE.md",
    "PULL_REQUEST_TEMPLATE.md",
    "README.md",
    "SECURITY.md",
    "SUPPORT.md",
];
const NAMESPACE_PREFIXES: &[&str] = &[".github/ISSUE_TEMPLATE/", "profile/", "workflow-templates/"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationErrors(Vec<String>);

enum RepositoryPath {
    Exists(fs::Metadata),
    Missing,
    Symlink,
    Escapes,
    Error(io::Error),
}

impl Display for ValidationErrors {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(formatter, "repository validation failed:")?;
        for error in &self.0 {
            writeln!(formatter, "- {error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ValidationErrors {}

#[derive(Clone, Copy)]
struct WorkflowContract {
    path: &'static str,
    branch: &'static str,
    reusable_workflow: &'static str,
}

impl WorkflowContract {
    fn reusable_reference(self) -> String {
        format!(
            "uses: {REUSABLE_WORKFLOW_OWNER}/{}@{REUSABLE_REVISION}",
            self.reusable_workflow
        )
    }

    fn lines(self) -> Vec<String> {
        vec![
            "name: Validate".into(),
            "on:".into(),
            "  pull_request:".into(),
            "  push:".into(),
            "    branches:".into(),
            format!("      - {}", self.branch),
            "permissions:".into(),
            "  contents: read".into(),
            "jobs:".into(),
            "  validate:".into(),
            format!("    {}", self.reusable_reference()),
        ]
    }
}

const WORKFLOW_CONTRACTS: &[WorkflowContract] = &[
    WorkflowContract {
        path: ".github/workflows/validate.yml",
        branch: "main",
        reusable_workflow: "reusable-rust-cargo-validate.yml",
    },
    WorkflowContract {
        path: "workflow-templates/documentation-validation.yml",
        branch: "$default-branch",
        reusable_workflow: "reusable-python-repository-validate.yml",
    },
    WorkflowContract {
        path: "workflow-templates/rust-validation.yml",
        branch: "$default-branch",
        reusable_workflow: "reusable-rust-cargo-validate.yml",
    },
];

pub fn run_validation() -> Result<(), Box<dyn std::error::Error>> {
    let root = workspace_root()?;
    for arguments in [
        vec!["+stable", "fmt", "--all", "--check"],
        vec![
            "+stable",
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked",
            "--",
            "-D",
            "warnings",
        ],
        vec![
            "+stable",
            "test",
            "--workspace",
            "--all-targets",
            "--locked",
        ],
    ] {
        run_cargo(&root, &arguments)?;
    }
    validate_repository(&root).map_err(|errors| Box::new(errors) as Box<dyn std::error::Error>)
}

pub fn validate_repository(root: &Path) -> Result<(), ValidationErrors> {
    validate_repository_with_after_snapshot(root, || {})
}

fn validate_repository_with_after_snapshot<F>(
    root: &Path,
    after_snapshot: F,
) -> Result<(), ValidationErrors>
where
    F: FnOnce(),
{
    let mut errors = Vec::new();
    let initial_fingerprint = match worktree_fingerprint(root) {
        Ok(fingerprint) => Some(fingerprint),
        Err(error) => {
            errors.push(format!("failed to capture repository state: {error}"));
            None
        }
    };
    after_snapshot();
    validate_required_paths(root, &mut errors);
    validate_text_files(root, &mut errors);
    validate_file_inventories(root, &mut errors);
    validate_public_contracts(root, &mut errors);
    validate_issue_forms(root, &mut errors);
    validate_workflows(root, &mut errors);
    validate_workflow_properties(root, &mut errors);

    if let Some(initial_fingerprint) = initial_fingerprint {
        match worktree_fingerprint(root) {
            Ok(final_fingerprint) if final_fingerprint != initial_fingerprint => {
                errors.push("repository changed during validation".to_owned());
            }
            Ok(_) => {}
            Err(error) => errors.push(format!("failed to capture repository state: {error}")),
        }
    }

    errors.sort();
    errors.dedup();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ValidationErrors(errors))
    }
}

fn worktree_fingerprint(root: &Path) -> Result<Vec<u8>, io::Error> {
    let status = run_git(
        root,
        &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
    )?;
    let tracked = run_git(root, &["ls-files", "--stage", "-z"])?;
    let mut fingerprint = Vec::with_capacity(status.stdout.len() + tracked.stdout.len() + 16);
    fingerprint.extend_from_slice(b"status\0");
    fingerprint.extend_from_slice(&status.stdout);
    fingerprint.extend_from_slice(b"tracked\0");
    fingerprint.extend_from_slice(&tracked.stdout);
    Ok(fingerprint)
}

fn run_git(root: &Path, arguments: &[&str]) -> Result<Output, io::Error> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(io::Error::other(format!(
            "git {} exited with {}: {}",
            arguments.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim(),
        )))
    }
}

fn workspace_root() -> Result<PathBuf, io::Error> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| io::Error::other("xtask is missing its workspace root"))
}

fn run_cargo(root: &Path, arguments: &[&str]) -> Result<(), io::Error> {
    let status = Command::new("cargo")
        .args(arguments)
        .current_dir(root)
        .status()?;
    successful_status("cargo", arguments, status)
}

fn successful_status(
    program: &str,
    arguments: &[&str],
    status: ExitStatus,
) -> Result<(), io::Error> {
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{program} {} exited with {status}",
            arguments.join(" ")
        )))
    }
}

fn validate_required_paths(root: &Path, errors: &mut Vec<String>) {
    let manifest = root.join("validation-required-files.txt");
    let Some(contents) = read_utf8(&manifest, root, errors) else {
        return;
    };

    for line in contents.lines() {
        let required = line.trim();
        if required.is_empty() || required.starts_with('#') {
            continue;
        }
        let path = root.join(required);
        match inspect_repository_path(root, &path) {
            RepositoryPath::Exists(metadata) if metadata.is_file() && metadata.len() > 0 => {}
            RepositoryPath::Exists(metadata) if metadata.is_file() => {
                errors.push(format!("{required}: required file is empty"))
            }
            RepositoryPath::Symlink | RepositoryPath::Escapes => {
                errors.push(format!("{required}: symbolic links are not permitted"))
            }
            RepositoryPath::Error(error) => {
                errors.push(format!("{required}: failed to inspect path: {error}"))
            }
            RepositoryPath::Exists(_) | RepositoryPath::Missing => {
                errors.push(format!("{required}: required file is missing"))
            }
        }
    }

    for forbidden in FORBIDDEN_PATHS {
        if !matches!(
            inspect_repository_path(root, &root.join(forbidden)),
            RepositoryPath::Missing
        ) {
            let reason = if *forbidden == RETIRED_VALIDATOR_PATH {
                "retired Python validator must not exist"
            } else {
                "retired issue form must not exist"
            };
            errors.push(format!("{forbidden}: {reason}"));
        }
    }
}

fn validate_text_files(root: &Path, errors: &mut Vec<String>) {
    let mut files = Vec::new();
    collect_files(root, root, &mut files, errors);
    for path in files {
        let Ok(relative) = relative_path(root, &path) else {
            errors.push(format!("{}: unsupported path encoding", path.display()));
            continue;
        };
        if is_text_path(&path) {
            validate_text_file(root, &path, &relative, errors);
        }
    }
}

fn collect_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<PathBuf>,
    errors: &mut Vec<String>,
) {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push(format!(
                "{}: failed to read directory: {error}",
                directory.display()
            ));
            return;
        }
    };
    let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        if relative
            .components()
            .next()
            .is_some_and(|component| component.as_os_str() == OsStr::new(".git"))
            || relative
                .components()
                .next()
                .is_some_and(|component| component.as_os_str() == OsStr::new("target"))
        {
            continue;
        }
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                errors.push(format!(
                    "{}: failed to inspect path: {error}",
                    path.display()
                ));
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            match relative_path(root, &path) {
                Ok(relative) => {
                    errors.push(format!("{relative}: symbolic links are not permitted"))
                }
                Err(()) => errors.push(format!("{}: unsupported path encoding", path.display())),
            }
        } else if metadata.is_file() {
            files.push(path);
        } else if metadata.is_dir() {
            collect_files(root, &path, files, errors);
        }
    }
}

fn is_text_path(path: &Path) -> bool {
    if path
        .file_name()
        .is_some_and(|name| name == OsStr::new("CODEOWNERS"))
    {
        return true;
    }
    path.extension()
        .and_then(OsStr::to_str)
        .map(|extension| TEXT_SUFFIXES.contains(&extension.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn validate_text_file(root: &Path, path: &Path, relative: &str, errors: &mut Vec<String>) {
    let bytes = match inspect_repository_path(root, path) {
        RepositoryPath::Exists(metadata) if metadata.is_file() => match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                errors.push(format!("{relative}: failed to read text: {error}"));
                return;
            }
        },
        RepositoryPath::Symlink | RepositoryPath::Escapes => {
            errors.push(format!("{relative}: symbolic links are not permitted"));
            return;
        }
        RepositoryPath::Missing | RepositoryPath::Exists(_) => return,
        RepositoryPath::Error(error) => {
            errors.push(format!("{relative}: failed to inspect path: {error}"));
            return;
        }
    };
    if bytes.contains(&0) {
        errors.push(format!("{relative}: contains a NUL byte"));
        return;
    }
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => {
            errors.push(format!("{relative}: is not valid UTF-8"));
            return;
        }
    };
    if !text.is_empty() && !text.ends_with('\n') {
        errors.push(format!("{relative}: must end with a newline"));
    }
    for (index, line) in text.lines().enumerate() {
        if line.ends_with([' ', '\t']) {
            errors.push(format!("{relative}:{}: trailing whitespace", index + 1));
        }
        if line.contains('\t') {
            errors.push(format!("{relative}:{}: tab character", index + 1));
        }
    }
    if is_namespace_surface(relative) {
        let lowered = text.to_ascii_lowercase();
        for token in ["github.com/crystonix/", "`crystonix/", "crystonix/runen"] {
            if lowered.contains(token) {
                errors.push(format!(
                    "{relative}: contains historical owner token {token:?}"
                ));
            }
        }
    }
    if path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
    {
        validate_markdown_links(root, path, relative, &text, errors);
    }
}

fn is_namespace_surface(relative: &str) -> bool {
    NAMESPACE_EXACT_PATHS.contains(&relative)
        || NAMESPACE_PREFIXES
            .iter()
            .any(|prefix| relative.starts_with(prefix))
}

fn validate_markdown_links(
    root: &Path,
    path: &Path,
    relative: &str,
    text: &str,
    errors: &mut Vec<String>,
) {
    for target in markdown_link_targets(text) {
        let raw_target = target.trim();
        let target = raw_target
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .trim_matches(['<', '>']);
        if target.is_empty() || target.starts_with('#') || is_external_target(target) {
            continue;
        }
        let file_target = target.split(['#', '?']).next().unwrap_or_default();
        if file_target.is_empty() {
            continue;
        }
        let decoded = percent_decode(file_target);
        match resolve_markdown_link(root, path.parent().unwrap_or(root), &decoded) {
            RepositoryPath::Exists(_) => {}
            RepositoryPath::Escapes => {
                errors.push(format!("{relative}: link escapes repository: {raw_target}"))
            }
            RepositoryPath::Symlink => errors.push(format!(
                "{relative}: symbolic links are not permitted: {raw_target}"
            )),
            RepositoryPath::Missing => {
                errors.push(format!("{relative}: broken relative link: {raw_target}"))
            }
            RepositoryPath::Error(error) => errors.push(format!(
                "{relative}: failed to inspect relative link {raw_target}: {error}"
            )),
        }
    }
}

fn markdown_link_targets(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut targets = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'[' {
            index += 1;
            continue;
        }
        let label_start = index + 1;
        let Some(label_end_offset) = bytes[label_start..].iter().position(|byte| *byte == b']')
        else {
            index += 1;
            continue;
        };
        let label_end = label_start + label_end_offset;
        if bytes.get(label_end + 1) != Some(&b'(') {
            index += 1;
            continue;
        }
        let target_start = label_end + 2;
        let Some(target_end_offset) = bytes[target_start..].iter().position(|byte| *byte == b')')
        else {
            index += 1;
            continue;
        };
        let target_end = target_start + target_end_offset;
        if let Some(target) = text.get(target_start..target_end) {
            targets.push(target);
        }
        index = target_end + 1;
    }
    targets
}

fn inspect_repository_path(root: &Path, path: &Path) -> RepositoryPath {
    let canonical_root = match root.canonicalize() {
        Ok(root) => root,
        Err(error) => return RepositoryPath::Error(error),
    };
    let relative = match path.strip_prefix(root) {
        Ok(relative) => relative,
        Err(_) => return RepositoryPath::Escapes,
    };
    let mut candidate = root.to_path_buf();
    for component in relative.components() {
        match component {
            Component::CurDir => continue,
            Component::Normal(segment) => candidate.push(segment),
            Component::ParentDir | Component::Prefix(_) | Component::RootDir => {
                return RepositoryPath::Escapes;
            }
        }
        let metadata = match fs::symlink_metadata(&candidate) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return RepositoryPath::Missing;
            }
            Err(error) if error.kind() == io::ErrorKind::NotADirectory => {
                return RepositoryPath::Missing;
            }
            Err(error) => return RepositoryPath::Error(error),
        };
        if metadata.file_type().is_symlink() {
            return match candidate.canonicalize() {
                Ok(target) if target.starts_with(&canonical_root) => RepositoryPath::Symlink,
                Ok(_) => RepositoryPath::Escapes,
                Err(_) => RepositoryPath::Symlink,
            };
        }
        if candidate == path {
            return RepositoryPath::Exists(metadata);
        }
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) => RepositoryPath::Exists(metadata),
        Err(error) if error.kind() == io::ErrorKind::NotFound => RepositoryPath::Missing,
        Err(error) => RepositoryPath::Error(error),
    }
}

fn is_external_target(target: &str) -> bool {
    let lower = target.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
}

fn percent_decode(target: &str) -> String {
    let mut decoded = Vec::with_capacity(target.len());
    let bytes = target.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
        {
            decoded.push(high * 16 + low);
            index += 3;
            continue;
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn resolve_markdown_link(root: &Path, source_directory: &Path, target: &str) -> RepositoryPath {
    if target.starts_with('/')
        || target.contains('\\')
        || target.starts_with("\\\\")
        || target.starts_with("//")
        || target.as_bytes().get(1) == Some(&b':')
    {
        return RepositoryPath::Escapes;
    }

    let canonical_root = match root.canonicalize() {
        Ok(root) => root,
        Err(error) => return RepositoryPath::Error(error),
    };
    let mut current = match source_directory.canonicalize() {
        Ok(directory) if directory.starts_with(&canonical_root) => directory,
        Ok(_) => return RepositoryPath::Escapes,
        Err(error) => return RepositoryPath::Error(error),
    };
    let components = target
        .split('/')
        .filter(|component| !component.is_empty() && *component != ".")
        .collect::<Vec<_>>();

    for (index, component) in components.iter().enumerate() {
        if *component == ".." {
            let Some(parent) = current.parent() else {
                return RepositoryPath::Escapes;
            };
            if !parent.starts_with(&canonical_root) {
                return RepositoryPath::Escapes;
            }
            current = parent.to_path_buf();
            continue;
        }

        let candidate = current.join(component);
        let metadata = match fs::symlink_metadata(&candidate) {
            Ok(metadata) => metadata,
            Err(error)
                if error.kind() == io::ErrorKind::NotFound
                    || error.kind() == io::ErrorKind::NotADirectory =>
            {
                return RepositoryPath::Missing;
            }
            Err(error) => return RepositoryPath::Error(error),
        };
        if metadata.file_type().is_symlink() {
            return match candidate.canonicalize() {
                Ok(destination) if destination.starts_with(&canonical_root) => {
                    RepositoryPath::Symlink
                }
                Ok(_) => RepositoryPath::Escapes,
                Err(_) => RepositoryPath::Symlink,
            };
        }

        if index + 1 == components.len() {
            return RepositoryPath::Exists(metadata);
        }
        if !metadata.is_dir() {
            return RepositoryPath::Missing;
        }
        current = match candidate.canonicalize() {
            Ok(directory) if directory.starts_with(&canonical_root) => directory,
            Ok(_) => return RepositoryPath::Escapes,
            Err(error) => return RepositoryPath::Error(error),
        }
    }

    match fs::symlink_metadata(&current) {
        Ok(metadata) => RepositoryPath::Exists(metadata),
        Err(error) if error.kind() == io::ErrorKind::NotFound => RepositoryPath::Missing,
        Err(error) => RepositoryPath::Error(error),
    }
}

fn validate_file_inventories(root: &Path, errors: &mut Vec<String>) {
    validate_exact_inventory(
        root,
        ".github/ISSUE_TEMPLATE",
        REQUIRED_ISSUE_TEMPLATES,
        "issue-template",
        errors,
    );
    validate_exact_inventory(
        root,
        "workflow-templates",
        REQUIRED_WORKFLOW_TEMPLATES,
        "workflow-template",
        errors,
    );

    let actual = files_directly_below(root, ".github/workflows", Some(&["yml", "yaml"]), errors);
    let expected = REQUIRED_ACTIVE_WORKFLOWS
        .iter()
        .map(|path| (*path).to_owned())
        .collect();
    if actual != expected {
        errors.push(format!(
            "active workflow inventory drift; expected {expected:?}, found {actual:?}"
        ));
    }
}

fn validate_exact_inventory(
    root: &Path,
    directory: &str,
    expected_paths: &[&str],
    kind: &str,
    errors: &mut Vec<String>,
) {
    let actual = files_directly_below(root, directory, None, errors);
    let expected = expected_paths
        .iter()
        .map(|path| (*path).to_owned())
        .collect();
    if actual != expected {
        errors.push(format!(
            "{kind} inventory mismatch; expected {expected:?}, found {actual:?}"
        ));
    }
}

fn files_directly_below(
    root: &Path,
    relative_directory: &str,
    extensions: Option<&[&str]>,
    errors: &mut Vec<String>,
) -> BTreeSet<String> {
    let directory = root.join(relative_directory);
    match inspect_repository_path(root, &directory) {
        RepositoryPath::Exists(metadata) if metadata.is_dir() => {}
        RepositoryPath::Symlink | RepositoryPath::Escapes => {
            errors.push(format!(
                "{relative_directory}: symbolic links are not permitted"
            ));
            return BTreeSet::new();
        }
        RepositoryPath::Missing | RepositoryPath::Exists(_) => return BTreeSet::new(),
        RepositoryPath::Error(error) => {
            errors.push(format!(
                "{relative_directory}: failed to inspect path: {error}"
            ));
            return BTreeSet::new();
        }
    }
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(_) => return BTreeSet::new(),
    };
    let mut paths = BTreeSet::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                errors.push(format!(
                    "{}: failed to inspect path: {error}",
                    path.display()
                ));
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            match relative_path(root, &path) {
                Ok(relative) => {
                    errors.push(format!("{relative}: symbolic links are not permitted"))
                }
                Err(()) => errors.push(format!("{}: unsupported path encoding", path.display())),
            }
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        if let Some(extensions) = extensions {
            let Some(extension) = path.extension().and_then(OsStr::to_str) else {
                continue;
            };
            if !extensions
                .iter()
                .any(|expected| extension.eq_ignore_ascii_case(expected))
            {
                continue;
            }
        }
        match relative_path(root, &path) {
            Ok(path) => {
                paths.insert(path);
            }
            Err(()) => errors.push(format!("{}: unsupported path encoding", path.display())),
        }
    }
    paths
}

fn validate_public_contracts(root: &Path, errors: &mut Vec<String>) {
    require_tokens(
        root,
        "CONTRIBUTING.md",
        &[
            "Do not begin a code or documentation contribution unless:",
            "An open issue is not by itself authorization to implement it.",
            "owner-only, discussion, or open-contribution mode",
        ],
        errors,
    );
    require_tokens(
        root,
        "GOVERNANCE.md",
        &[
            "dornglut/engineering/blob/main/governance/authority-and-work.md",
            "dornglut/engineering/blob/main/standards/github.md",
            "GitHub Projects represent operational priority and status.",
        ],
        errors,
    );
    require_tokens(
        root,
        "CODE_OF_CONDUCT.md",
        &[
            "## Reporting",
            "## Enforcement",
            "confidentiality cannot be guaranteed",
        ],
        errors,
    );
    require_tokens(
        root,
        "SECURITY.md",
        &[
            "Do not report suspected vulnerabilities",
            "private vulnerability report",
        ],
        errors,
    );
    require_tokens(
        root,
        "PULL_REQUEST_TEMPLATE.md",
        &[
            "Accepted implementation base, when relevant:",
            "Reviewed feature head:",
            "Synthetic merge-result revision, only when separately validated:",
            "Accepted squash merge, after merge:",
            "Accepted-main push revision or run, when required:",
            "Repository profile or organization-policy exception:",
        ],
        errors,
    );
}

fn validate_issue_forms(root: &Path, errors: &mut Vec<String>) {
    require_tokens(
        root,
        ".github/ISSUE_TEMPLATE/config.yml",
        &[
            "blank_issues_enabled: false",
            "https://github.com/dornglut/.github/security/policy",
            "https://github.com/dornglut/engineering/issues/new",
        ],
        errors,
    );
    require_tokens(
        root,
        ".github/ISSUE_TEMPLATE/defect.yml",
        &[
            "name: Defect",
            "title: \"[Defect] \"",
            "id: repository_revision",
            "id: observed",
            "id: expected",
            "id: reproduction",
            "id: environment",
            "id: evidence",
            "does not disclose a security vulnerability, credential, or private data",
        ],
        errors,
    );
    require_tokens(
        root,
        ".github/ISSUE_TEMPLATE/proposal.yml",
        &[
            "name: Proposal",
            "title: \"[Proposal] \"",
            "id: proposal_type",
            "- Capability",
            "- Architecture",
            "- Documentation",
            "- Tooling",
            "- Research",
            "id: repository",
            "id: problem",
            "id: outcome",
            "id: evidence",
            "id: boundaries",
            "private Dornglut Inbox",
        ],
        errors,
    );
}

fn require_tokens(root: &Path, relative: &str, tokens: &[&str], errors: &mut Vec<String>) {
    let Some(text) = read_utf8(&root.join(relative), root, errors) else {
        return;
    };
    for token in tokens {
        if !text.contains(token) {
            errors.push(format!(
                "{relative}: missing required contract token {token:?}"
            ));
        }
    }
}

fn validate_workflows(root: &Path, errors: &mut Vec<String>) {
    for contract in WORKFLOW_CONTRACTS {
        let Some(text) = read_utf8(&root.join(contract.path), root, errors) else {
            continue;
        };
        workflow_contract_failures(contract.path, &text, *contract, errors);
    }
}

fn workflow_contract_failures(
    path: &str,
    text: &str,
    contract: WorkflowContract,
    errors: &mut Vec<String>,
) {
    let lines = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.trim_end().to_owned())
        .collect::<Vec<_>>();
    let expected = contract.lines();
    if lines.first() != expected.first() {
        errors.push(format!(
            "{path}: workflow identity drift; expected {:?}",
            expected[0]
        ));
    }
    if lines.get(1..6) != Some(&expected[1..6]) {
        if !lines
            .get(1..6)
            .is_some_and(|trigger| trigger.contains(&expected[5]))
        {
            errors.push(format!(
                "{path}: workflow branch drift; expected {:?}",
                expected[5]
            ));
        }
        errors.push(format!("{path}: workflow trigger drift; expected unconfigured pull_request and push for {:?} only", contract.branch));
    }
    if lines.get(6..8) != Some(&expected[6..8]) {
        errors.push(format!(
            "{path}: workflow permission drift; expected top-level contents: read only"
        ));
    }
    let references = lines
        .iter()
        .filter(|line| line.trim_start().starts_with("uses:"))
        .map(|line| line.trim().to_owned())
        .collect::<Vec<_>>();
    let expected_reference = contract.reusable_reference();
    if references != [expected_reference.clone()] {
        for reference in &references {
            if !has_full_sha(reference) {
                errors.push(format!("{path}: immutable revision drift; reusable workflow reference is not pinned to a full commit SHA: {reference:?}"));
            }
        }
        errors.push(format!("{path}: workflow profile or immutable revision drift; expected {:?}, found {references:?}", expected_reference));
    }
    if lines.get(8..) != Some(&expected[8..]) {
        errors.push(format!(
            "{path}: workflow job drift; expected sole reusable job 'validate'"
        ));
    }
    if lines != expected {
        let unexpected = lines
            .iter()
            .filter(|line| !expected.contains(*line))
            .collect::<Vec<_>>();
        if !unexpected.is_empty() {
            errors.push(format!(
                "{path}: unexpected workflow field or ordering drift: {unexpected:?}"
            ));
        }
    }
}

fn has_full_sha(reference: &str) -> bool {
    reference.rsplit_once('@').is_some_and(|(_, revision)| {
        revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

fn validate_workflow_properties(root: &Path, errors: &mut Vec<String>) {
    let directory = root.join("workflow-templates");
    match inspect_repository_path(root, &directory) {
        RepositoryPath::Exists(metadata) if metadata.is_dir() => {}
        RepositoryPath::Symlink | RepositoryPath::Escapes => {
            errors.push("workflow-templates: symbolic links are not permitted".to_owned());
            return;
        }
        RepositoryPath::Missing | RepositoryPath::Exists(_) => return,
        RepositoryPath::Error(error) => {
            errors.push(format!(
                "workflow-templates: failed to inspect path: {error}"
            ));
            return;
        }
    }
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    let mut properties = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.ends_with(".properties.json"))
        })
        .collect::<Vec<_>>();
    properties.sort();
    for path in properties {
        let Ok(relative) = relative_path(root, &path) else {
            errors.push(format!("{}: unsupported path encoding", path.display()));
            continue;
        };
        let value = match read_utf8(&path, root, errors)
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        {
            Some(value) => value,
            None => {
                errors.push(format!("{relative}: invalid JSON"));
                continue;
            }
        };
        for key in ["name", "description", "iconName"] {
            if value
                .get(key)
                .and_then(Value::as_str)
                .is_none_or(|value| value.trim().is_empty())
            {
                errors.push(format!("{relative}: {key} must be a non-empty string"));
            }
        }
        for key in ["categories", "filePatterns"] {
            let valid = value
                .get(key)
                .and_then(Value::as_array)
                .is_some_and(|items| {
                    !items.is_empty()
                        && items
                            .iter()
                            .all(|item| item.as_str().is_some_and(|value| !value.is_empty()))
                });
            if !valid {
                errors.push(format!(
                    "{relative}: {key} must be a non-empty list of non-empty strings"
                ));
            }
        }
        let workflow = path.with_file_name(
            path.file_name()
                .and_then(OsStr::to_str)
                .unwrap_or_default()
                .trim_end_matches(".properties.json")
                .to_owned()
                + ".yml",
        );
        if !matches!(inspect_repository_path(root, &workflow), RepositoryPath::Exists(metadata) if metadata.is_file())
        {
            errors.push(format!("{relative}: matching workflow template is missing"));
        }
    }
}

fn read_utf8(path: &Path, root: &Path, errors: &mut Vec<String>) -> Option<String> {
    let relative = relative_path(root, path).unwrap_or_else(|()| path.display().to_string());
    match inspect_repository_path(root, path) {
        RepositoryPath::Exists(metadata) if metadata.is_file() => match fs::read(path) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => Some(text),
                Err(_) => {
                    errors.push(format!("{relative}: failed to read UTF-8 text"));
                    None
                }
            },
            Err(error) => {
                errors.push(format!("{relative}: failed to read UTF-8 text: {error}"));
                None
            }
        },
        RepositoryPath::Symlink | RepositoryPath::Escapes => {
            errors.push(format!("{relative}: symbolic links are not permitted"));
            None
        }
        RepositoryPath::Missing | RepositoryPath::Exists(_) => {
            errors.push(format!(
                "{relative}: failed to read UTF-8 text: file is missing"
            ));
            None
        }
        RepositoryPath::Error(error) => {
            errors.push(format!("{relative}: failed to inspect path: {error}"));
            None
        }
    }
}

fn relative_path(root: &Path, path: &Path) -> Result<String, ()> {
    path.strip_prefix(root)
        .map_err(|_| ())?
        .to_str()
        .map(|path| path.replace('\\', "/"))
        .ok_or(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn repository_state_changes_between_snapshots_are_rejected() {
        let root = std::env::temp_dir().join(format!(
            "dornglut-validation-state-change-{}-{}",
            std::process::id(),
            TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let status = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&root)
            .status()
            .unwrap();
        assert!(status.success(), "git init failed: {status}");
        fs::write(root.join("tracked.md"), b"before\n").unwrap();
        let status = Command::new("git")
            .args(["add", "tracked.md"])
            .current_dir(&root)
            .status()
            .unwrap();
        assert!(status.success(), "git add failed: {status}");

        let failure = validate_repository_with_after_snapshot(&root, || {
            fs::write(root.join("tracked.md"), b"after\n").unwrap();
        })
        .unwrap_err()
        .to_string();
        assert!(failure.contains("repository changed during validation"));

        fs::remove_dir_all(root).unwrap();
    }
}

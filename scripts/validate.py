#!/usr/bin/env python3
"""Read-only validation for Dornglut organization defaults."""

from __future__ import annotations

import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[1]
TEXT_SUFFIXES = {".md", ".yml", ".yaml", ".txt", ".py", ".json"}
LINK_RE = re.compile(r"!?\[[^\]]*\]\(([^)]+)\)")
FULL_SHA_RE = re.compile(r"@[0-9a-f]{40}$")
REUSABLE_REVISION = "624cb41adeed21a6461eb838bc7330bd0a5079fd"
REUSABLE_WORKFLOW_OWNER = "dornglut/github-workflows/.github/workflows"

ISSUE_TEMPLATE_FILES = {
    ".github/ISSUE_TEMPLATE/config.yml",
    ".github/ISSUE_TEMPLATE/defect.yml",
    ".github/ISSUE_TEMPLATE/proposal.yml",
}
WORKFLOW_TEMPLATE_FILES = {
    "workflow-templates/documentation-validation.properties.json",
    "workflow-templates/documentation-validation.yml",
    "workflow-templates/rust-validation.properties.json",
    "workflow-templates/rust-validation.yml",
}
ACTIVE_WORKFLOW_FILES = {".github/workflows/validate.yml"}
FORBIDDEN_PATHS = {
    ".github/ISSUE_TEMPLATE/architecture.yml",
    ".github/ISSUE_TEMPLATE/bug.yml",
    ".github/ISSUE_TEMPLATE/feature.yml",
}
NAMESPACE_EXACT_PATHS = {
    "README.md",
    "AGENTS.md",
    "CODE_OF_CONDUCT.md",
    "CONTRIBUTING.md",
    "GOVERNANCE.md",
    "PULL_REQUEST_TEMPLATE.md",
    "SECURITY.md",
    "SUPPORT.md",
}
NAMESPACE_PREFIXES = (
    ".github/ISSUE_TEMPLATE/",
    "profile/",
    "workflow-templates/",
)


@dataclass(frozen=True)
class WorkflowContract:
    path: str
    branch: str
    reusable_workflow: str

    @property
    def reusable_reference(self) -> str:
        return (
            f"uses: {REUSABLE_WORKFLOW_OWNER}/"
            f"{self.reusable_workflow}@{REUSABLE_REVISION}"
        )

    @property
    def lines(self) -> tuple[str, ...]:
        return (
            "name: Validate",
            "on:",
            "  pull_request:",
            "  push:",
            "    branches:",
            f"      - {self.branch}",
            "permissions:",
            "  contents: read",
            "jobs:",
            "  validate:",
            f"    {self.reusable_reference}",
        )


WORKFLOW_CONTRACTS = (
    WorkflowContract(
        path=".github/workflows/validate.yml",
        branch="main",
        reusable_workflow="reusable-python-repository-validate.yml",
    ),
    WorkflowContract(
        path="workflow-templates/documentation-validation.yml",
        branch="$default-branch",
        reusable_workflow="reusable-python-repository-validate.yml",
    ),
    WorkflowContract(
        path="workflow-templates/rust-validation.yml",
        branch="$default-branch",
        reusable_workflow="reusable-rust-cargo-validate.yml",
    ),
)


def fail(message: str, failures: list[str]) -> None:
    failures.append(message)


def relative(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def read_text(path: Path, failures: list[str]) -> str | None:
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as error:
        fail(f"{relative(path)}: failed to read UTF-8 text: {error}", failures)
        return None


def is_namespace_surface(path_text: str) -> bool:
    return path_text in NAMESPACE_EXACT_PATHS or path_text.startswith(NAMESPACE_PREFIXES)


def validate_text_file(path: Path, failures: list[str]) -> None:
    data = path.read_bytes()
    path_text = relative(path)

    if b"\x00" in data:
        fail(f"{path_text}: contains a NUL byte", failures)
        return

    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        fail(f"{path_text}: is not valid UTF-8", failures)
        return

    if text and not text.endswith("\n"):
        fail(f"{path_text}: must end with a newline", failures)

    for line_number, line in enumerate(text.splitlines(), start=1):
        if line.endswith((" ", "\t")):
            fail(f"{path_text}:{line_number}: trailing whitespace", failures)
        if "\t" in line:
            fail(f"{path_text}:{line_number}: tab character", failures)

    if is_namespace_surface(path_text):
        lowered = text.lower()
        for token in ("github.com/crystonix/", "`crystonix/", "crystonix/runen"):
            if token in lowered:
                fail(f"{path_text}: contains historical owner token {token!r}", failures)

    if path.suffix.lower() != ".md":
        return

    for match in LINK_RE.finditer(text):
        raw_target = match.group(1).strip()
        target = raw_target.split(maxsplit=1)[0].strip("<>")
        if not target or target.startswith(("#", "http://", "https://", "mailto:")):
            continue

        target = unquote(target.split("#", 1)[0].split("?", 1)[0])
        if not target:
            continue

        resolved = (path.parent / target).resolve()
        try:
            resolved.relative_to(ROOT)
        except ValueError:
            fail(f"{path_text}: link escapes repository: {raw_target}", failures)
            continue

        if not resolved.exists():
            fail(f"{path_text}: broken relative link: {raw_target}", failures)


def validate_required_paths(failures: list[str]) -> None:
    manifest = ROOT / "validation-required-files.txt"
    if not manifest.is_file():
        fail("validation-required-files.txt: missing", failures)
        return

    for raw_line in manifest.read_text(encoding="utf-8").splitlines():
        required = raw_line.strip()
        if not required or required.startswith("#"):
            continue
        path = ROOT / required
        if not path.is_file():
            fail(f"{required}: required file is missing", failures)
        elif path.stat().st_size == 0:
            fail(f"{required}: required file is empty", failures)

    for forbidden in sorted(FORBIDDEN_PATHS):
        if (ROOT / forbidden).exists():
            fail(f"{forbidden}: retired issue form must not exist", failures)


def files_below(path: Path) -> set[str]:
    if not path.is_dir():
        return set()
    return {relative(candidate) for candidate in path.iterdir() if candidate.is_file()}


def validate_file_inventories(failures: list[str]) -> None:
    issue_files = files_below(ROOT / ".github" / "ISSUE_TEMPLATE")
    if issue_files != ISSUE_TEMPLATE_FILES:
        fail(
            "issue-template inventory mismatch; "
            f"expected {sorted(ISSUE_TEMPLATE_FILES)}, found {sorted(issue_files)}",
            failures,
        )

    workflow_files = files_below(ROOT / "workflow-templates")
    if workflow_files != WORKFLOW_TEMPLATE_FILES:
        fail(
            "workflow-template inventory mismatch; "
            f"expected {sorted(WORKFLOW_TEMPLATE_FILES)}, found {sorted(workflow_files)}",
            failures,
        )


def require_tokens(path: str, tokens: tuple[str, ...], failures: list[str]) -> None:
    file_path = ROOT / path
    text = read_text(file_path, failures)
    if text is None:
        return
    for token in tokens:
        if token not in text:
            fail(f"{path}: missing required contract token {token!r}", failures)


def validate_public_contracts(failures: list[str]) -> None:
    require_tokens(
        "CONTRIBUTING.md",
        (
            "Do not begin a code or documentation contribution unless:",
            "An open issue is not by itself authorization to implement it.",
            "owner-only, discussion, or open-contribution mode",
        ),
        failures,
    )
    require_tokens(
        "GOVERNANCE.md",
        (
            "dornglut/engineering/blob/main/governance/authority-and-work.md",
            "dornglut/engineering/blob/main/standards/github.md",
            "GitHub Projects represent operational priority and status.",
        ),
        failures,
    )
    require_tokens(
        "CODE_OF_CONDUCT.md",
        ("## Reporting", "## Enforcement", "confidentiality cannot be guaranteed"),
        failures,
    )
    require_tokens(
        "SECURITY.md",
        ("Do not report suspected vulnerabilities", "private vulnerability report"),
        failures,
    )
    require_tokens(
        "PULL_REQUEST_TEMPLATE.md",
        (
            "Accepted implementation base, when relevant:",
            "Reviewed feature head:",
            "Synthetic merge-result revision, only when separately validated:",
            "Accepted squash merge, after merge:",
            "Accepted-main push revision or run, when required:",
            "Repository profile or organization-policy exception:",
        ),
        failures,
    )


def validate_issue_forms(failures: list[str]) -> None:
    require_tokens(
        ".github/ISSUE_TEMPLATE/config.yml",
        (
            "blank_issues_enabled: false",
            "https://github.com/dornglut/.github/security/policy",
            "https://github.com/dornglut/engineering/issues/new",
        ),
        failures,
    )
    require_tokens(
        ".github/ISSUE_TEMPLATE/defect.yml",
        (
            "name: Defect",
            'title: "[Defect] "',
            "id: repository_revision",
            "id: observed",
            "id: expected",
            "id: reproduction",
            "id: environment",
            "id: evidence",
            "does not disclose a security vulnerability, credential, or private data",
        ),
        failures,
    )
    require_tokens(
        ".github/ISSUE_TEMPLATE/proposal.yml",
        (
            "name: Proposal",
            'title: "[Proposal] "',
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
        ),
        failures,
    )


def validate_active_workflow_inventory(failures: list[str]) -> None:
    workflow_dir = ROOT / ".github" / "workflows"
    active_workflows = (
        {
            relative(candidate)
            for candidate in workflow_dir.iterdir()
            if candidate.is_file() and candidate.suffix.lower() in {".yml", ".yaml"}
        }
        if workflow_dir.is_dir()
        else set()
    )
    if active_workflows != ACTIVE_WORKFLOW_FILES:
        fail(
            "active workflow inventory drift; "
            f"expected {sorted(ACTIVE_WORKFLOW_FILES)}, found {sorted(active_workflows)}",
            failures,
        )


def normalize_workflow_lines(text: str) -> tuple[str, ...]:
    return tuple(line.rstrip() for line in text.splitlines() if line.strip())


def workflow_contract_failures(
    path: str, text: str, contract: WorkflowContract
) -> list[str]:
    lines = normalize_workflow_lines(text)
    expected = contract.lines
    failures: list[str] = []

    if lines[:1] != expected[:1]:
        failures.append(
            f"{path}: workflow identity drift; expected {expected[0]!r}"
        )

    actual_events = lines[1:6]
    expected_events = expected[1:6]
    if actual_events != expected_events:
        if expected[5] not in actual_events:
            failures.append(
                f"{path}: workflow branch drift; expected {expected[5]!r}"
            )
        failures.append(
            f"{path}: workflow trigger drift; expected unconfigured pull_request "
            f"and push for {contract.branch!r} only"
        )

    if lines[6:8] != expected[6:8]:
        failures.append(
            f"{path}: workflow permission drift; expected top-level contents: read only"
        )

    references = [line.strip() for line in lines if line.strip().startswith("uses:")]
    if references != [contract.reusable_reference]:
        for reference in references:
            if not FULL_SHA_RE.search(reference):
                failures.append(
                    f"{path}: immutable revision drift; reusable workflow reference "
                    f"is not pinned to a full commit SHA: {reference!r}"
                )
        failures.append(
            f"{path}: workflow profile or immutable revision drift; "
            f"expected {contract.reusable_reference!r}, found {references!r}"
        )

    if lines[8:] != expected[8:]:
        failures.append(
            f"{path}: workflow job drift; expected sole reusable job 'validate'"
        )

    if lines != expected:
        unexpected = [line for line in lines if line not in expected]
        if unexpected:
            failures.append(
                f"{path}: unexpected workflow field or ordering drift: {unexpected!r}"
            )

    return failures


def validate_workflows(failures: list[str]) -> None:
    for contract in WORKFLOW_CONTRACTS:
        text = read_text(ROOT / contract.path, failures)
        if text is not None:
            failures.extend(workflow_contract_failures(contract.path, text, contract))


def validate_workflow_properties(failures: list[str]) -> None:
    for path in sorted((ROOT / "workflow-templates").glob("*.properties.json")):
        path_text = relative(path)
        try:
            value = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
            fail(f"{path_text}: invalid JSON: {error}", failures)
            continue

        for key in ("name", "description", "iconName"):
            if not isinstance(value.get(key), str) or not value[key].strip():
                fail(f"{path_text}: {key} must be a non-empty string", failures)

        for key in ("categories", "filePatterns"):
            if not isinstance(value.get(key), list) or not value[key]:
                fail(f"{path_text}: {key} must be a non-empty list", failures)
            elif not all(isinstance(item, str) and item for item in value[key]):
                fail(f"{path_text}: {key} entries must be non-empty strings", failures)

        workflow_name = path.name.removesuffix(".properties.json") + ".yml"
        if not (path.parent / workflow_name).is_file():
            fail(f"{path_text}: matching workflow template is missing", failures)


def main() -> int:
    failures: list[str] = []

    validate_required_paths(failures)

    for path in sorted(ROOT.rglob("*")):
        if not path.is_file() or ".git" in path.parts:
            continue
        if path.suffix.lower() in TEXT_SUFFIXES or path.name in {"CODEOWNERS"}:
            validate_text_file(path, failures)

    validate_file_inventories(failures)
    validate_active_workflow_inventory(failures)
    validate_public_contracts(failures)
    validate_issue_forms(failures)
    validate_workflows(failures)
    validate_workflow_properties(failures)

    if failures:
        print("repository validation failed:", file=sys.stderr)
        for failure in sorted(set(failures)):
            print(f"- {failure}", file=sys.stderr)
        return 1

    print("repository validation passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

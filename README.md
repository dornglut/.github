# Dornglut organization defaults

This repository supplies broadly applicable GitHub community-health defaults and workflow templates for repositories owned by the Dornglut organization.

It does not own product behavior, architecture, validation semantics, releases, roadmaps, or live work state. Those remain with the repository responsible for the software.

## Inheritance

Repository-local community-health files override the corresponding organization default.

Issue templates are a suite-level exception: when a repository contains a local `.github/ISSUE_TEMPLATE` directory, it must provide its complete accepted issue-template suite because GitHub does not merge that directory with the organization defaults.

The normative organization model lives in:

- [Dornglut authority and work](https://github.com/dornglut/engineering/blob/main/governance/authority-and-work.md);
- [Dornglut GitHub standard](https://github.com/dornglut/engineering/blob/main/standards/github.md);
- [Dornglut repository standard](https://github.com/dornglut/engineering/blob/main/standards/repositories.md).

Canonical validation:

```text
cargo validate
```

## License

The current repository revision is licensed under the [Apache License 2.0](LICENSE) (`Apache-2.0`).

Before this migration, no repository-wide `LICENSE` or `LICENSE.md` file was published in the inspected Git history. This migration therefore does not infer or reinterpret a repository-wide license grant for earlier revisions.

<!-- atomic-multifile-canary-v2: root -->

# Agent instructions

Scope: organization-wide GitHub defaults and workflow templates only.

Start with:

1. `README.md` for inheritance boundaries;
2. `CONTRIBUTING.md`, `SECURITY.md`, `SUPPORT.md`, and `GOVERNANCE.md` for public defaults;
3. `.github/ISSUE_TEMPLATE/` and `PULL_REQUEST_TEMPLATE.md` for intake contracts;
4. the accepted standards in `dornglut/engineering`.

Rules:

- do not place product architecture, implementation plans, release state, roadmaps, priorities, or generated Project data here;
- keep defaults broadly applicable and conservative;
- remember that a local issue-template directory replaces the organization suite rather than extending it;
- keep validation read-only and limited to this repository;
- do not add organization secrets, deployment credentials, source-writing automation, or floating reusable-workflow references;
- update `validation-required-files.txt` whenever a required path changes;
- run `cargo validate` before proposing changes.

Canonical validation operates on a stable checkout: do not edit, rename, replace, relink, or otherwise write repository paths while `cargo validate` runs. Concurrent mutation invalidates the result. Static symlinks are rejected, but validation does not provide hostile-local-process isolation.

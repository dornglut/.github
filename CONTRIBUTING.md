# Contributing

These defaults apply when a repository does not provide its own contribution guide.

## Contribution boundary

Issues, evidence, questions, and concrete proposals are welcome where the affected repository enables them.

Do not begin a code or documentation contribution unless:

- the repository explicitly states that external contributions are accepted; or
- a maintainer authorizes the work in an issue.

An open issue is not by itself authorization to implement it. Repositories may operate in owner-only, discussion, or open-contribution mode.

## Before authorized work

1. Read the repository README, architecture entrypoints, accepted decision records, contribution policy, and local `AGENTS.md`.
2. Confirm the owning issue, intended outcome, boundaries, acceptance evidence, and non-goals.
3. Branch from the current accepted default branch. Do not stack work on an unmerged implementation branch unless explicitly authorized.

## Change discipline

- Keep one pull request focused on one coherent outcome.
- Preserve repository-local ownership boundaries.
- Do not introduce forwarding packages, duplicate sources of truth, compatibility aliases, generated-state authority, or hidden workflow side effects without an accepted decision.
- Keep validation and ordinary CI read-only with respect to implementation source.
- Update active documentation when behavior, ownership, commands, or public contracts change.
- Preserve historical provenance as history rather than rewriting it as current authority.

## Validation

Run the repository's canonical validation command. The pull request records:

- the exact command;
- the exact tested head;
- the result;
- any intentionally deferred checks and their owner.

Passing a narrower command does not replace the repository validation authority.

## Pull requests

A pull request states:

- the owning issue or accepted decision;
- what changed and why;
- included and excluded scope;
- validation evidence;
- migration, compatibility, security, and operational implications;
- the next safe action.

Use a draft pull request while implementation or evidence is incomplete.

# Contributing

These defaults apply when a repository does not provide its own contribution guide.

## Before changing code or documentation

1. Read the repository README, architecture documents, accepted decision records, and local `AGENTS.md`.
2. Use an existing issue for nontrivial work, or create one that defines the outcome, boundaries, acceptance evidence, and non-goals.
3. Branch from the current accepted default branch. Do not stack new work on an unmerged implementation branch unless the repository explicitly authorizes it.

## Change discipline

- Keep one pull request focused on one coherent outcome.
- Preserve repository-local ownership boundaries.
- Do not introduce forwarding packages, duplicate sources of truth, compatibility aliases, generated-state authority, or hidden workflow side effects without an accepted decision.
- Keep automation read-only with respect to implementation source unless a repository explicitly defines a different trusted release workflow.
- Update active documentation when behavior, ownership, commands, or public contracts change. Preserve historical provenance as history rather than rewriting it as current authority.

## Validation

Run the repository's canonical validation command. The pull request must record:

- the exact command;
- the exact tested head;
- the result;
- any intentionally deferred checks and their owner.

Passing a narrower command does not replace the repository validation authority.

## Pull requests

A pull request should state:

- the owning issue or accepted decision;
- what changed and why;
- scope and explicit non-scope;
- validation evidence;
- migration, compatibility, security, and operational implications;
- the next safe action.

Draft pull requests are expected while implementation or evidence is incomplete.

# Governance

Dornglut separates shared organizational policy from repository implementation authority.

## Authority order

1. code and tests define current behavior;
2. the repository's canonical validation command defines its merge baseline;
3. accepted repository decision records define durable local architecture;
4. repository issues define active implementation work;
5. [Dornglut Engineering](https://github.com/dornglut/engineering) defines cross-repository policy, shared architecture, and organization-level decisions;
6. GitHub Projects may represent live priority, sequencing, and dates but do not replace durable decisions.

A more specific repository-local rule overrides this default when the two conflict.

## Ownership

Each repository owns its implementation, public API, release policy, validation semantics, and local roadmap. Organization repositories may provide defaults and orchestration, but they must not silently change product behavior.

Organization owners administer access and policy. Merge authority remains repository-specific and should be protected by required validation and review rules appropriate to the repository's maturity.

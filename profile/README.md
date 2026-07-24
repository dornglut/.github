# Dornglut

Dornglut develops reusable software foundations, creative tools, and the Runen framework family.

## Runen repositories

- [Runenwerk](https://github.com/dornglut/runenwerk) — integration platform and reference engine
- [RunenUI](https://github.com/dornglut/runen-ui) — host-neutral UI framework
- [RunenSDF](https://github.com/dornglut/runen-sdf) — standalone signed-distance-field framework

Planned framework repositories include RunenGPU, RunenRender, and RunenECS. A planned name does not imply a completed extraction or public contract.

## Engineering model

- repository code and tests define current behavior;
- each repository owns its validation semantics, public contracts, releases, local architecture, roadmap, and issues;
- nontrivial accepted work is issue-owned and delivered through a pull request;
- undeveloped ideas are kept separate from accepted public work;
- cross-repository policy, standards, and architecture live in [Dornglut Engineering](https://github.com/dornglut/engineering);
- reusable read-only CI orchestration lives in [Dornglut GitHub Workflows](https://github.com/dornglut/github-workflows).

Historical owner paths may remain in explicit provenance records. Active work uses the `dornglut/*` namespace.

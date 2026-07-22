# Dornglut

Dornglut develops reusable software foundations, creative tools, and the Runen framework family.

## Runen repositories

- [Runenwerk](https://github.com/dornglut/runenwerk) — integration platform and reference engine
- [RunenUI](https://github.com/dornglut/runen-ui) — host-neutral UI framework
- [RunenSDF](https://github.com/dornglut/runen-sdf) — standalone signed-distance-field framework

Planned framework repositories include RunenGPU, RunenRender, and RunenECS.

## Engineering model

- repository code and tests define current behavior;
- each repository owns one read-only validation command;
- nontrivial work is tracked by an issue and delivered through a pull request;
- cross-repository policy and architecture live in [Dornglut Engineering](https://github.com/dornglut/engineering);
- shared CI orchestration lives in [Dornglut GitHub Workflows](https://github.com/dornglut/github-workflows).

Historical owner paths may remain in provenance records. Active work uses the `dornglut/*` namespace.

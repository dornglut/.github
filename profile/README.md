# Dornglut

Dornglut builds the **Runen ecosystem**: independently useful Rust frameworks, an integration engine, a programming language, and human-first engineering tools.

Repositories are separated by semantic ownership rather than gathered behind one monolith. Standalone frameworks own their contracts and validation; Runenwerk composes accepted framework boundaries into applications and tools; organization policy and reusable CI remain separate from product code.

## Runen ecosystem

### Integration and language

- [Runenwerk](https://github.com/dornglut/runenwerk) — experimental Rust integration platform and reference engine for world-centric applications, editors, simulations, and rendering systems.
- [Runen](https://github.com/dornglut/runen) — programming language under semantic-kernel development.

### Standalone frameworks

- [RunenSDF](https://github.com/dornglut/runen-sdf) — host-neutral signed-field mathematics and deterministic CPU reference queries.
- [RunenUI](https://github.com/dornglut/runen-ui) — host- and renderer-neutral UI framework with a deterministic pre-1.0 headless foundation.
- [RunenNet](https://github.com/dornglut/runen-net) — engine-independent realtime multiplayer networking framework.
- [RunenOnline](https://github.com/dornglut/runen-online) — provider-neutral online-game control-plane framework, separate from realtime networking.
- **RunenSpatial** — host-neutral spatial identities, addressing, indexing, deterministic demand, and availability control. The repository is currently private while standalone conformance, release policy, and Runenwerk cutover work are completed.

### Applications and experiments

- [Werkstatt](https://github.com/dornglut/werkstatt) — experimental human-first engineering workbench for understanding, performing, reviewing, and coordinating software work across human and automated actors.
- [Runen Lab](https://github.com/dornglut/runen-lab) — downstream experimental and showcase workspace for applications built on accepted Runen surfaces; it does not define framework semantics or canonical cross-framework integration.

## Framework boundaries not yet standalone

**RunenGPU, RunenRender, RunenECS, and RunenScheduler are not yet standalone framework authorities.** Their implementation and extraction authority remains with Runenwerk until the corresponding clean-cutover gates are completed.

The public `dornglut/runen-ecs` repository currently reserves the namespace only; its existence does not represent a completed extraction, independently usable implementation, or stable public contract. Likewise, a planned framework or repository name does not by itself authorize source movement or imply maturity.

## Engineering infrastructure

- [Dornglut Engineering](https://github.com/dornglut/engineering) — organization governance, standards, cross-repository architecture, ADRs, initiatives, and audits.
- [Dornglut GitHub Workflows](https://github.com/dornglut/github-workflows) — reusable read-only CI orchestration that invokes repository-owned validation.
- [Organization defaults](https://github.com/dornglut/.github) — public profile, inherited community-health defaults, issue and pull-request templates, and workflow templates.

## Engineering model

- one semantic invariant set has one authority;
- each repository owns its code, tests, public contracts, validation semantics, releases, local architecture, roadmap, and issues;
- standalone frameworks do not depend on Runenwerk merely to define their semantics;
- Runenwerk owns product integration and adapters rather than duplicating accepted framework authority;
- reusable CI orchestrates repository-owned validation instead of redefining it centrally;
- nontrivial accepted work is issue-owned, reviewed through a pull request, and validated at the exact reviewed head;
- framework extraction uses clean cutovers: prove the standalone boundary, migrate real consumers, then delete duplicate source and compatibility authority.

Historical owner paths may remain in explicit provenance records. Active authority and links use the `dornglut/*` namespace.

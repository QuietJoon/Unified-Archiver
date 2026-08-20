# Architecture Index

* [Architecture: unified-archive](README.md) - Maintainer note: this directory is design and implementation material, not the release-facing contract.
* [ADR / DCR corpus design review — revert, improve, innovate](adr-dcr-design-review-2026-07-19.md) - Full-corpus critique of all 93 decision records (55 architecture ADs, 30 review-gate MADRs, 8 DCRs) at small and big focus; revert candidates prioritized.
* [Bootstrap Config](bootstrap-config.md) - This document describes how the project is run locally, how processes are started, and where environment / configuration is sourced from.
* [Config Surface](config-surface.md) - Configuration categories, ownership, and usage.
* [ADR / DCR design review — revert / improve / innovate](decision-review-2026-07-19.md) - Cross-cutting critique of all 93 decision records (57 architecture ADs + 30 review-gate MADRs + 8 DCRs), prioritising reversible decisions. Design-only; no code changes made.
* [Dictionary](dictionary.md) - Project terminology for unified-archive.
* [Effort and Risk](effort-and-risk.md) - Dependency-ordered implementation slices with risk notes.
* [MVP Scope: unified-archive v0.1.0](mvp-scope.md) - List archive contents with metadata via unified API (ZIP, RAR/RAR5, 7z, TAR variants, ISO)
* [Persistence and Files](persistence-and-files.md) - State persistence and file lifecycle design for unified-archive.
* [Rough Schema](rough-schema.md) - In-memory entity structure for unified-archive.
* [Scenario Matrix](scenario-matrix.md) - Scenario-to-system mapping for all mandatory MVP scenarios.
* [Verification Matrix](verification-matrix.md) - Scenario-to-verification mapping.
* [Walkthroughs](walkthroughs.md) - End-to-end architectural walkthroughs for mandatory scenarios.
* [ZIP backend comparison: piz vs the `zip` crate (dual-backend revert input)](zip-piz-backend-comparison.md) - Advisory. Read-only investigation feeding the AD 0007 dual-ZIP-backend revert decision.

The investigation snapshot formerly kept here as `architecture_investigation.md` /
`codebase_investigation.md` now lives in the sharded, regenerable
[`docs/investigation/`](../investigation/README.md) tree and is not part of this bundle.

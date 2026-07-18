# Open-Source Launch Checklist

The plan for TPT Vertex's public open-source launch. Check items off as they are
completed; the launch is go when all **Required** items are done.

## Versioning policy

- TPT Vertex follows **Semantic Versioning** (`MAJOR.MINOR.PATCH`).
- Pre-1.0 (`0.y.z`): breaking changes bump the **minor** version; features and
  fixes bump the **patch** version. APIs may change while we stabilize.
- Post-1.0: breaking changes bump **major**; backward-compatible features bump
  **minor**; fixes bump **patch**.
- All crates in the workspace share the workspace version for coherence.
- Every release has a git tag `vX.Y.Z` and a changelog entry.

## Required before launch

### Legal & licensing

- [x] Dual **MIT OR Apache-2.0** license files present (`LICENSE-MIT`,
      `LICENSE-APACHE`) and `NOTICE`.
- [x] SPDX headers/expressions in source and manifests.
- [ ] `cargo deny`/license scan confirms all dependencies are compatible.

### Repository hygiene

- [x] `README.md` with pitch, architecture, and build instructions.
- [x] `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`.
- [x] Issue and PR templates.
- [x] ADRs for major decisions.
- [ ] Branding: final name confirmed (TPT Vertex ✓), **logo** produced, domain
      live (tpt-vertex.dev).
- [ ] Package/crate names reserved on crates.io and npm.

### Quality gates

- [x] CI: Rust build/test/clippy/fmt; frontend build/test/lint/format.
- [x] Desktop packaging workflow (multi-OS).
- [ ] End-to-end workflow test(s) green in CI.
- [ ] Security review actions triaged; **critical/high** items resolved
      (see [security-review.md](security-review.md)).
- [ ] Accessibility pass follow-ups triaged
      (see [accessibility.md](accessibility.md)).

### Documentation

- [x] User guide, plugin/API docs, ADRs, security & accessibility notes.
- [ ] Documentation site published from `docs/`.

### Community

- [x] Community & support page and contributor onboarding guide.
- [ ] Community channels created (Discussions enabled; chat invite live).

## Launch day

- [ ] Tag the release (`vX.Y.Z`) and publish release notes.
- [ ] Publish crates/npm packages (if publishing).
- [ ] Publish desktop installers as release assets.
- [ ] Make the repository public.
- [ ] Announce (blog post, social, relevant open-hardware communities).
- [ ] Monitor Issues/Discussions for the first 48 hours and triage quickly.

## Post-launch

- [ ] Establish a regular release cadence.
- [ ] Set up Dependabot and periodic `cargo audit`/`npm audit`.
- [ ] Grow maintainers; document governance as the community forms.
- [ ] Collect feedback and prioritize the next milestone.

## Release notes template

```
## vX.Y.Z — YYYY-MM-DD

### Highlights
- ...

### Added
- ...

### Changed
- ...

### Fixed
- ...

### Breaking changes
- ...
```

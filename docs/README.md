# TPT Vertex Documentation

Welcome to the TPT Vertex documentation — the source for the user guide,
architecture, and developer/plugin references. This directory is structured so it
can be published as a static documentation site (e.g. with MkDocs, Docusaurus, or
mdBook) with minimal changes; each page is a standalone Markdown file.

> Building the site: point your static-site generator at this `docs/` directory.
> The suggested navigation is the "Contents" below. Until a generator is wired
> up in CI, these pages are readable directly on the repository host.

## Contents

### User guide

- [User Guide](user-guide.md) — install, the workspace, sketching, features,
  assemblies, versioning, collaboration, and export.

### Architecture & decisions

- [Architecture Decision Records](adr/README.md) — the "why" behind major
  technical choices (representation, versioning, CRDT, desktop).

### Developer & API

- [Public API & Plugin Interface](plugin-api.md) — crate map, the plugin traits
  for custom formats/tools, and built-in formats.

### Operations & quality

- [Security Review](security-review.md) — threat model and hardening actions.
- [Accessibility](accessibility.md) — WCAG posture and follow-ups.

### Community & release

- [Community & Support](community.md) — where to get help and how to participate.
- [Contributor Onboarding](contributor-onboarding.md) — first-contribution guide
  for external contributors.
- [Launch Checklist](launch-checklist.md) — the open-source launch plan and
  versioning policy.

## Contributing to the docs

Docs live alongside the code. Improvements are welcome via pull request — see
[CONTRIBUTING.md](../CONTRIBUTING.md). Keep pages focused, link generously, and
prefer examples over prose.

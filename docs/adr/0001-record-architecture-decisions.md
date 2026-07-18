# ADR-0001: Record architecture decisions

- Status: Accepted
- Date: 2026-07-18

## Context

As TPT Vertex grows across a Rust geometry kernel, a WebGPU renderer, a React
frontend, a desktop client, collaboration sync, and version control, we need a
lightweight, durable way to record the significant architectural and technology
decisions — and the reasoning behind them — so future contributors understand
*why* things are the way they are without digging through chat history.

## Decision

We will record architecture decisions as Architecture Decision Records (ADRs)
in `docs/adr/`, following Michael Nygard's pattern
(<https://cognitect.com/blog/2011-11-10/6739229-record-architecture-decisions>).

- Each ADR is a single Markdown file, numbered sequentially.
- ADRs are immutable once `Accepted`; changes are captured by a new superseding
  ADR (`Superseded by ADR-XXXX`).
- The `README.md` in this folder maintains the index.

## Consequences

- Decisions and their rationale are documented in-repo and reviewable via PRs.
- New contributors can onboard by reading the ADR index.
- Discipline is required to actually write ADRs for significant choices.

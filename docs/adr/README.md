# Architecture Decision Records (ADRs)

This directory holds the Architecture Decision Records for TPT Vertex, following
the lightweight process described in
[ADR-0001](0001-record-architecture-decisions.md) (based on Michael Nygard's
*Record architecture decisions*).

## What is an ADR?

An Architecture Decision Record is a short, focused document that captures a
single, important, architecturally significant decision: the context, the
options considered, the decision made, and its consequences. ADRs are immutable
once accepted — superseded decisions are marked with a new ADR, never rewritten.

## How to propose an ADR

1. Copy `template.md` to `NNNN-short-title.md`, where `NNNN` is the next number.
2. Fill in the sections: Status, Context, Decision, Consequences.
3. Open a pull request. Discuss and refine.
4. On acceptance, set `Status: Accepted`. On rejection, set `Status: Rejected`.

## Index

| Number | Title                                              | Status   |
| ------ | -------------------------------------------------- | -------- |
| 0001   | Record architecture decisions                      | Accepted |
| 0002   | Dual MIT OR Apache-2.0 licensing                   | Accepted |
| 0003   | Monorepo layout (kernel / frontend / desktop)      | Accepted |
| 0004   | Geometric representation: hybrid B-rep + CSG       | Accepted |
| 0004   | Geometric representation: hybrid B-rep + CSG       | Accepted |

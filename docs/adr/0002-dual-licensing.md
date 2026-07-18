# ADR-0002: Dual MIT OR Apache-2.0 licensing

- Status: Accepted
- Date: 2026-07-18

## Context

TPT Vertex aims to maximize adoption across open-source hardware communities,
commercial products, and contributors. We want permissive terms that are easy to
combine with other projects, while also providing explicit patent granting and
attribution clarity for organizations that expect Apache-2.0.

## Decision

TPT Vertex is dual-licensed under **MIT OR Apache-2.0**, at the user's option.
This is expressed via the SPDX expression `MIT OR Apache-2.0` in `Cargo.toml`
(`license`) and `package.json` (`license`), accompanied by `LICENSE-MIT`,
`LICENSE-APACHE`, and a `NOTICE` file. New source files carry an
`SPDX-License-Identifier: MIT OR Apache-2.0` header.

## Consequences

- Users may pick the license that best fits their use case.
- Apache-2.0's patent grant and NOTICE requirements apply to those who choose it.
- Downstream projects can comply by satisfying either license.

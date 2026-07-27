# Vendored MCP specification schema — provenance

## Artifact

| Field | Value |
|---|---|
| Path | `spec/2025-11-25/schema.json` |
| Upstream repo | <https://github.com/modelcontextprotocol/modelcontextprotocol> |
| Upstream path | `schema/2025-11-25/schema.json` |
| Pinned commit | `7634684382c3d14cf7e9f14073fe40a2d8ace3fa` (committed 2026-07-23T23:49:30Z) |
| Fetch URL | `https://raw.githubusercontent.com/modelcontextprotocol/modelcontextprotocol/7634684382c3d14cf7e9f14073fe40a2d8ace3fa/schema/2025-11-25/schema.json` |
| Fetch date | 2026-07-26 |
| SHA-256 | `7b2d96fd95efd2216aa953606b83f5a740ddeaa5ebd3a5d27b45a8296545a118` |
| Size | 174,326 bytes |
| JSON Schema dialect | `https://json-schema.org/draft/2020-12/schema` |
| Type definitions | 145, under the top-level `$defs` key |

The artifact is pinned to a commit SHA rather than `main`. Re-fetching the URL above
reproduces this file byte for byte; re-fetching from `main` does not.

## Licence

The upstream repository is mid-transition between licences, and GitHub's licence API
reports `NOASSERTION` for it. Its `LICENSE` file opens with the following preamble,
quoted verbatim rather than paraphrased:

> The MCP project is undergoing a licensing transition from the MIT License to the
> Apache License, Version 2.0 ("Apache-2.0"). All new code and specification
> contributions to the project are licensed under Apache-2.0. Documentation
> contributions (excluding specifications) are licensed under CC-BY-4.0.
>
> Contributions for which relicensing consent has been obtained are licensed under
> Apache-2.0. Contributions made by authors who originally licensed their work under
> the MIT License and who have not yet granted explicit permission to relicense remain
> licensed under the MIT License.
>
> No rights beyond those granted by the applicable original license are conveyed for
> such contributions.

So the vendored file is not covered by a single SPDX identifier. As a *specification*
contribution it falls under the Apache-2.0 clause of that preamble, except for any
portion authored by a contributor who has not granted relicensing consent, which remains
MIT.

mcpkit itself is licensed `MIT OR Apache-2.0` (see `LICENSE-MIT` and `LICENSE-APACHE`).

## Why this exists

Every test in this workspace that is named for "compliance" asserts mcpkit's types
against mcpkit's own constants. This file is the first external spec artifact in the
repo. It is consumed by `scripts/schema-diff.sh`.

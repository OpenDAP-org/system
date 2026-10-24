# Documentation

| Folder | Contents |
|---|---|
| [`requirements/`](requirements/) | What this project must achieve. Each requirement has an ID, a priority (**must** / **should**) and a status. |
| [`research/`](research/) | Research reports and benchmarks that feed requirements and ADRs. |
| [`specs/`](specs/) | Specifications: the system architecture, the Rust crate architecture, user-facing conventions, [input events](specs/input-events.md). |
| [`verification/`](verification/) | What was verified and how: the [combinatorial matrix of the composer's primitives](verification/primitives-matrix.md), one case per row with its evidence, rendered from `primitives-matrix.toml` by `render.py`. |
| [`adr/`](adr/) | Architecture Decision Records: decisions already taken, with their context. A decision can be superseded by a later ADR. |

Only information confirmed by the maintainers is recorded here. Earlier experiments are not a source.

**Requirement status:** *Confirmed* (agreed, build on it) · *Draft* (agreed intent, wording or detail still open).
**ADR status:** *Accepted* · *Proposed* (direction agreed, not final) · *Superseded*.

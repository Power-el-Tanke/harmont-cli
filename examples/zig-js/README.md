# zig + js parallel demo

This example exists to make harmont's chain-level parallelism
visible. The pipeline declares one shared `apt-base` step that
both a Zig install chain and a Node install chain build off of.
The orchestrator places those two chains in independent slots in
its chain DAG, so once `apt-base` commits its snapshot the Zig and
Node installs run in two parallel Docker containers — each
booting from the same apt-base snapshot.

## Run it

```sh
hm run ci
```

## What to look for

In the human-format output, the two language-install steps emit
their `start` events within milliseconds of each other and their
`end` events with overlapping wall-clock spans:

```
[:apt: base] start (runner=docker)
[:apt: base] end exit=0 duration=11234ms
[:zig: install] start (runner=docker)
[:node: install] start (runner=docker)         ← starts before zig-install ends
[:zig: install] end exit=0 duration=27411ms
[:node: install] end exit=0 duration=18302ms
```

If the two `start` lines are separated by more than ~100ms, the
scheduler did not parallelise the chains — check `--parallelism`
(default = CPU count) and that the two install steps share no
`depends_on` edge in the rendered v0 IR.

## How the diamond is built

The pipeline registers three `@hm.target()`s and one
`@hm.pipeline("ci")`. The `apt_base` target is declared as a
parameter on both `zig_project` and `web_project`; harmont-py's
fixture-style DI passes the same memoized `Step` to both, so the
emitted IR contains exactly one apt-base node and both language
installs carry `builds_in: <apt-base key>`. Memoization is the
mechanism — there is no special "fork" primitive at the DSL level.

## Subprojects

- `zig-src/` — a tiny Zig library + test, copied from
  `examples/zig/`.
- `web/` — a tiny TypeScript library + vitest, copied from
  `examples/typescript/`.

Both are built and tested by their respective toolchains
(`hm.zig`, `hm.npm`) using the standard action methods.

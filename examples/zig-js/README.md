# zig + js parallel demo (with shared toolchain install)

Three patterns demonstrated together:

1. **apt-base fork.** One apt-base step commits a snapshot; two
   language-install chains (zig + node) boot from it in parallel
   containers.
2. **Shared toolchain install.** One `:zig: install` step is
   shared by two zig sub-projects (`zig-a/`, `zig-b/`). The IR
   contains exactly one install node; both project chains fan out
   from it.
3. **Chain-level parallelism.** Every chain with no `builds_in` or
   `depends_on` edge between it and a sibling runs concurrently,
   bounded by `--parallelism` (default = CPU count).

## Run it

```sh
hm run ci
```

## What to look for

- Exactly one `[:zig: install] start`/`end` pair — never two.
- `[:zig: install] start` and `[:node: install] start` appear
  within ~10ms of `[:apt: base] end` (the apt-base fork).
- After `[:zig: install] end`, the two project chains
  (`:zig: zig-a build/test` and `:zig: zig-b build/test`) start
  in parallel.

## How the diamond is built

`hm.zig()` (no `path`) returns a `ZigToolchain` holding the shared
install Step. `tc.project(path="zig-a")` and `tc.project(path="zig-b")`
both wrap the same Step, so the emitted IR contains exactly one
install node. The `@hm.target()` memoization is what makes the
shared toolchain show up exactly once — the toolchain factory is
called once per render and the returned `ZigToolchain` is reused.

## Subprojects

- `zig-a/` — adds two ints.
- `zig-b/` — subtracts two ints.
- `web/` — a tiny TypeScript library + vitest.

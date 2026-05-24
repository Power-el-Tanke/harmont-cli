import {
  pipeline,
  push,
  pullRequest,
  aptBase,
  type PipelineDefinition,
} from "harmont";
import { rust, py } from "harmont/toolchains";

const ALL_APT = [
  "curl",
  "ca-certificates",
  "build-essential",
  "pkg-config",
  "libssl-dev",
  "python3",
  "python3-venv",
] as const;

const base = aptBase({ packages: ALL_APT });
const rustProject = rust({ path: ".", base });
const pyProject = py.uv({ path: "dsls/harmont-py", base });

const warm = rustProject.warmup();

const pipelines: PipelineDefinition[] = [
  {
    slug: "ci",
    triggers: [push({ branch: "main" }), pullRequest({ branches: ["main"] })],
    pipeline: pipeline(
      warm.sh(
        `. $HOME/.cargo/env && cd . && cargo test --workspace --locked --no-fail-fast`,
        { label: ":rust: test" },
      ),
      warm.sh(
        `. $HOME/.cargo/env && cd . && cargo clippy --workspace --tests --locked -- -D warnings`,
        { label: ":rust: clippy" },
      ),
      rustProject.fmt(),
      pyProject.lint(),
      pyProject.fmt(),
      pyProject.typecheck({ paths: "harmont" }),
      pyProject.run(
        "pytest -v --deselect tests/test_gradle.py --deselect tests/test_haskell.py",
        { label: ":python: test" },
      ),
      { env: { CI: "true" }, defaultImage: "ubuntu:24.04" },
    ),
  },
];

export default pipelines;

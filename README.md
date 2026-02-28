# Hawk

[![Hawk](https://img.shields.io/badge/Hawk-dev-blue)](https://hawk.dev)
[![CI](https://github.com/darkwingrick/hawk/actions/workflows/run_tests.yml/badge.svg)](https://github.com/darkwingrick/hawk/actions/workflows/run_tests.yml)

Welcome to Hawk, a high-performance code editor forked from [Zed](https://github.com/zed-industries/zed).

---

### Installation

On macOS, Linux, and Windows you can [download Hawk directly](https://hawk.dev/download) or build it from source.

### Developing Hawk

- [Building Hawk for macOS](./docs/src/development/macos.md)
- [Building Hawk for Linux](./docs/src/development/linux.md)
- [Building Hawk for Windows](./docs/src/development/windows.md)

### Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for ways you can contribute to Hawk.

### Licensing

License information for third party dependencies must be correctly provided for CI to pass.

We use [`cargo-about`](https://github.com/EmbarkStudios/cargo-about) to automatically comply with open source licenses. If CI is failing, check the following:

- Is it showing a `no license specified` error for a crate you've created? If so, add `publish = false` under `[package]` in your crate's Cargo.toml.
- Is the error `failed to satisfy license requirements` for a dependency? If so, first determine what license the project has and whether this system is sufficient to comply with this license's requirements. If you're unsure, ask a lawyer. Once you've verified that this system is acceptable add the license's SPDX identifier to the `accepted` array in `script/licenses/hawk-licenses.toml`.
- Is `cargo-about` unable to find the license for a dependency? If so, add a clarification field at the end of `script/licenses/hawk-licenses.toml`, as specified in the [cargo-about book](https://embarkstudios.github.io/cargo-about/cli/generate/config.html#crate-configuration).

## Project Links

- [Documentation](https://github.com/darkwingrick/hawk/tree/master/docs)
- [Issue Tracker](https://github.com/darkwingrick/hawk/issues)

# hok (Turbo Edition)

> Hok is a high-performance CLI implementation of [Scoop](https://scoop.sh/) written in Rust.

[![cicd][cicd-badge]][cicd]
[![crates-svg]][crates-url]
[![license][license-badge]](LICENSE)

---

💡 **About this fork:** This is a revived, fully functional, and accelerated fork of the original `hok` project, which was abandoned in 2024. All core commands that were previously marked as "not implemented" are now fully operational.

## What's New
* **Fully Working Core:** Implemented `install`, `upgrade`, and `cleanup` commands from scratch.
* **New Feature:** Added a completely new `status` command to track package states.
* **Modernized:** Updated the Rust toolchain and all underlying dependencies for better performance and security.

## Install

Assuming you have the original Scoop installed, you can build Hok from source or install it via your custom bucket (if applicable):

```sh
# Building from source
git clone [https://github.com/nidara-duo/hok](https://github.com/nidara-duo/hok)
cd hok
cargo install --path .

```

## Commands

The command line interface is fully compatible with Scoop.

```raw
$ hok help
Hok is a CLI implementation of Scoop in Rust

Usage: hok.exe <COMMAND>

Commands:
  bucket     Manage manifest buckets
  cache      Package cache management
  cat        Inspect the manifest of a package
  cleanup    Cleanup apps by removing old versions
  config     Configuration management
  hold       Hold package(s) to disable changes
  home       Browse the homepage of a package
  info       Show package(s) basic information
  install    Install package(s)
  list       List installed package(s)
  search     Search available package(s)
  status     Show status of packages and environment
  unhold     Unhold package(s) to enable changes
  uninstall  Uninstall package(s)
  update     Fetch and update subscribed buckets
  upgrade    Upgrade installed package(s)
  help       Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version

Type 'hok help <command>' to get help for a specific command.

```

## Development

Prerequisites: Git, Rust (latest stable)

```sh
# clone the repo
git clone [https://github.com/nidara-duo/hok](https://github.com/nidara-duo/hok)
cd hok
# build
cargo build --release
# run and test
cargo run -- help

```

## Performance

Hok (powered by the optimized `libscoop` backend) provides a blazingly fast alternative to the original Scoop written in PowerShell.

```sh
# Benchmarking scoop bucket list (Original baseline)
Benchmark 1: scoop bucket list
  Time (mean ± σ):      5.610 s ±  0.627 s

Benchmark 2: hok bucket list
  Time (mean ± σ):     159.4 ms ±  28.3 ms

Summary
  hok bucket list ran ~35 times faster than scoop bucket list

```

## License & Credits

* **Current Maintainer:** [Nidara Duo](https://github.com/nidara-duo)
* **Original Author:** [Chawye Hsu](https://github.com/chawyehsu) (Original code base up to 2024)

This project is released under the [Apache-2.0](https://www.google.com/search?q=LICENSE) license. For licenses of sub-crates, see [COPYING](https://www.google.com/search?q=COPYING).


## Roadmap / To-Do

### 📅 Near Future (Short-term)
- [ ] **UI/UX Enhancements:** Improve terminal output aesthetics and make table layouts much cleaner and more readable (e.g., using specialized Rust crates for CLI tables).

### 🚀 Long-term Plans
- [ ] **Nushell Integration:** Provide native support and deeper integration with [Nushell](https://www.nushell.sh/) to maximize execution speed and support structured data pipelines.

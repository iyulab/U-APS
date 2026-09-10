# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Maintained from 1.0.3 onward; earlier entries list release dates only (see git
history and the GitHub releases for those).

Versions here are the `UAPSVersion` shared by the `UAPS.SDK` and `UAPS.CLI`
NuGet packages. The Rust engine carries its own crate version and is not
published to a registry — it ships inside those packages.

## [Unreleased]

### Added

- `samples/` — runnable CLI inputs, each small enough to check by hand, with the
  input schema documented alongside them. The README described a samples
  directory and three build scripts that did not exist; the scripts are gone
  from it and the samples now do.

### Fixed

- An operation that names worker candidates got them in the order they were
  written, not by when they were free. With two operators available, a second
  operation would take the one already busy and wait for them, serialising work
  that had idle capacity on both sides — a two-machine, two-operator cell ran at
  half its throughput. Equipment selection already chose the earliest-available
  candidate, and so did worker selection when no candidate list was given; only
  the listed-candidates path differed. Reachable through JSON and FFI input
  only: the builder API cannot express worker candidates, which is why no test
  had ever covered that branch.

## [1.0.3] - 2026-09-07

### Added

- `UAPS.CLI`'s first run downloads the native engine for the current platform,
  and that was written down nowhere. A .NET tool package is runtime-identifier
  neutral, so unlike the SDK package it cannot carry the engine binaries
  itself. The README now states this, names the cache directory, and gives two
  ways to avoid the download.

### Fixed

- The offline first-run path reported `A task was canceled` instead of the
  guidance written for it. `HttpClient` surfaces its own timeout as a
  cancellation rather than an `HttpRequestException`, so that case escaped the
  only `catch`. It is now caught and guarded on the caller's token, so a
  genuine caller-initiated cancellation still propagates unchanged.

### Changed

- The engine binary in these packages is rebuilt against `rand` 0.10 and now
  declares `rust-version = "1.87"`, verified by building on that toolchain. No
  public signature is added or removed, which is why this is a patch release.
- Assertions in the test suite move from `FluentAssertions` 7.0.0 to
  `AwesomeAssertions` 9.6.0. Version 8 of the former changed from Apache 2.0 to
  a proprietary licence; the community fork keeps an OSI licence and a path to
  current releases. Test-only — no shipped code is affected.
- Test tooling: `coverlet.collector` 10.0.1, `Microsoft.NET.Test.Sdk` 18.9.0,
  `xunit` 2.9.3, `xunit.runner.visualstudio` 4.0.0.
- CI installs the .NET SDK version the projects actually target. It had pinned
  9.0.x against `net10.0` projects and passed only because the runner image
  happens to also ship .NET 10.

## [1.0.2] - 2026-09-03

`UAPS.SDK` bundles the native engine binaries in the package under
`runtimes/{rid}/native/` instead of downloading them on first use. Release
distribution consolidated into this repository.

## [1.0.1] - 2026-09-03

## [1.0.0] - 2026-09-03

# UAPS.CLI

Command line tool for the U-APS production scheduling engine. Reads a JSON or
Excel workbook describing jobs and resources, and writes the resulting schedule
in the same format.

## Install

```bash
dotnet tool install -g UAPS.CLI
```

## Usage

```bash
uaps input.json output.json
uaps input.xlsx output.xlsx

uaps input.json output.json --strategy SPT
uaps input.json output.json --strategy EDD --tie-breaker FIFO
```

## The first run needs network access

A .NET tool package is runtime-identifier neutral, so this package cannot carry
the native engine the way `UAPS.SDK` does. On its first run the tool downloads
the engine binary for the current platform into:

| Platform | Directory |
|---|---|
| Windows | `%LOCALAPPDATA%\UAPS\native` |
| Linux, macOS | `~/.local/share/UAPS/native` |

Every later run uses that copy and works offline.

To install onto a machine without network access, either place the binary for
the platform in that directory beforehand (available from the repository's
releases), or use `UAPS.SDK` instead — that package ships the engine for every
supported platform and never downloads anything.

## Requirements

- .NET 10.0 or later
- Windows x64, Linux x64, macOS x64, or macOS arm64

## Links

- [Repository and documentation](https://github.com/iyulab/U-APS)
- [Changelog](https://github.com/iyulab/U-APS/blob/main/CHANGELOG.md)
- Library package: `UAPS.SDK`

Licensed under MIT.

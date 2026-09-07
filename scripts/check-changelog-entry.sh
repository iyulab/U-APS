#!/usr/bin/env bash
#
# UAPSVersion must have a matching heading in CHANGELOG.md.
#
# release.yml already refuses to publish a version that is on nuget.org, so a
# forgotten bump cannot ship twice. The opposite gap is the one left open: a
# bump that *is* new publishes fine while its entry is still sitting under
# `## [Unreleased]`, leaving consumers of both packages with no record of what
# they upgraded into. Nothing else in this repository compares the version
# against the changelog.
#
# The version lives in sdk/Directory.Build.props rather than in a package
# manifest, because the two published packages (UAPS.SDK, UAPS.CLI) share one
# version and release.yml watches that file as its trigger.
#
# Run locally before pushing a version bump:
#   bash scripts/check-changelog-entry.sh
#
# Enforced in CI (.github/workflows/ci.yml, "Changelog Entry" job). A mismatch is
# emitted as a GitHub Actions ::error:: annotation so it surfaces inline.
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

props="sdk/Directory.Build.props"

version="$(grep -m1 -oE '<UAPSVersion>[^<]+</UAPSVersion>' "${props}" \
  | sed -E 's|</?UAPSVersion>||g')"
if [[ -z "${version}" ]]; then
  echo "::error file=${props}::could not read <UAPSVersion>"
  exit 1
fi
echo "UAPSVersion: ${version}"

# Escape the dots so `1.0.3` cannot match a heading like `1X0Y3`.
if grep -qE "^## \[${version//./\.}\]" CHANGELOG.md; then
  echo "Changelog has an entry for ${version}"
  exit 0
fi

echo "::error file=CHANGELOG.md::no '## [${version}]' heading — add the entry for this release in the same commit as the UAPSVersion bump (move it out of '## [Unreleased]')"
exit 1

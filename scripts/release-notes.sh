#!/usr/bin/env bash
#
# The GitHub Release body for a version: its CHANGELOG.md section, then how to
# install it.
#
# The release workflow used to write a body of installation steps only, so
# every release's notes were pasted in from the changelog by hand afterwards.
# The changelog is where a release's notes are written; this reads them from
# there.
#
#   bash scripts/release-notes.sh 1.6.0 > notes.md
#
# Fails when CHANGELOG.md has no `## [<version>]` section, or an empty one.
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
version="${1:?usage: release-notes.sh <version>}"

section="$(awk -v v="$version" '
  index($0, "## [" v "]") == 1 { inside = 1; next }
  inside && /^## \[/ { exit }
  inside { print }
' "${ROOT}/CHANGELOG.md" | sed -e '/./,$!d')"
# Trailing blank lines.
section="$(printf '%s\n' "${section}" | sed -e :a -e '/^\n*$/{$d;N;ba' -e '}')"

if [[ -z "${section//[[:space:]]/}" ]]; then
  echo "::error file=CHANGELOG.md::no '## [${version}]' section with notes" >&2
  exit 1
fi

cat <<EOF
## UAPS v${version}

${section}

Full history: [CHANGELOG.md](https://github.com/iyulab/U-APS/blob/main/CHANGELOG.md)

### Installation

#### CLI (dotnet tool - Recommended)
\`\`\`bash
dotnet tool install -g UAPS.CLI
\`\`\`

#### CLI (Quick Install - Self-contained)
\`\`\`bash
# Windows (PowerShell)
irm https://raw.githubusercontent.com/iyulab/U-APS/main/install.ps1 | iex

# Linux/macOS
curl -fsSL https://raw.githubusercontent.com/iyulab/U-APS/main/install.sh | bash
\`\`\`

#### SDK (NuGet)
\`\`\`bash
dotnet add package UAPS.SDK --version ${version}
\`\`\`

### Manual Download

#### Native Libraries
- \`uaps_engine-win-x64.dll\` - Windows x64
- \`uaps_engine-linux-x64.so\` - Linux x64
- \`uaps_engine-osx-x64.dylib\` - macOS x64
- \`uaps_engine-osx-arm64.dylib\` - macOS ARM64

#### CLI Bundles
- \`uaps-cli-win-x64.zip\` - Windows x64
- \`uaps-cli-linux-x64.tar.gz\` - Linux x64
- \`uaps-cli-osx-x64.tar.gz\` - macOS x64
- \`uaps-cli-osx-arm64.tar.gz\` - macOS ARM64
EOF

param(
    [string]$Version = "",
    [string]$Binary = "target/release/texstudio.exe"
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$manifest = Get-Content -LiteralPath (Join-Path $repoRoot "Cargo.toml") -Raw
$packageVersion = [regex]::Match($manifest, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
if (-not $Version) { $Version = $packageVersion }
if ($Version -notmatch '^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$' -or $Version -ne $packageVersion) {
    throw "Release version must match the package version in Cargo.toml."
}

$source = Join-Path $repoRoot $Binary
if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw "Build the Windows executable first." }
$dist = Join-Path $repoRoot "dist"
New-Item -ItemType Directory -Force -Path $dist | Out-Null
$name = "TexStudio-$Version-windows-x86_64"
$exe = Join-Path $dist "$name.exe"
$zip = Join-Path $dist "$name.zip"
$hashes = Join-Path $dist "$name-SHA256SUMS.txt"
foreach ($destination in @($exe, $zip, $hashes)) {
    if (Test-Path -LiteralPath $destination) { throw "Release output already exists: $destination" }
}
Copy-Item -LiteralPath $source -Destination $exe

# Explicit package allowlist: never include settings, projects, logs, or build caches.
$staging = Join-Path ([IO.Path]::GetTempPath()) ("TexStudio-package-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $staging | Out-Null
try {
    Copy-Item -LiteralPath $source -Destination (Join-Path $staging "TexStudio.exe")
    foreach ($file in @("LICENSE", "README.md", "THIRD_PARTY_NOTICES.md")) {
        Copy-Item -LiteralPath (Join-Path $repoRoot $file) -Destination $staging
    }
    $licenses = Join-Path $staging "licenses"
    New-Item -ItemType Directory -Path $licenses | Out-Null
    Copy-Item -LiteralPath (Join-Path $repoRoot "assets/pdfjs/LICENSE") -Destination (Join-Path $licenses "PDFjs-LICENSE.txt")
    Copy-Item -LiteralPath (Join-Path $repoRoot "assets/snippets/LaTeX-Workshop-LICENSE.txt") -Destination $licenses
    Copy-Item -LiteralPath (Join-Path $repoRoot "assets/completions/Aerijo-LICENSE.txt") -Destination $licenses
    Compress-Archive -Path (Join-Path $staging "*") -DestinationPath $zip
} finally {
    # Only the unique temporary directory created above may be removed recursively.
    $resolvedStaging = [IO.Path]::GetFullPath($staging)
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $resolvedStaging.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or
        (Split-Path -Leaf $resolvedStaging) -notmatch '^TexStudio-package-[0-9a-f-]{36}$') {
        throw "Refusing to clean a path outside the package staging directory."
    }
    Remove-Item -LiteralPath $resolvedStaging -Recurse -Force
}
$lines = foreach ($file in @($exe, $zip)) {
    $digest = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()
    "$digest  $(Split-Path -Leaf $file)"
}
# GNU sha256sum on the Linux publish runner treats CR as part of a filename.
[IO.File]::WriteAllText($hashes, ($lines -join "`n") + "`n", [Text.UTF8Encoding]::new($false))
Write-Output "Packaged $name.exe, .zip, and SHA256SUMS.txt in dist/."

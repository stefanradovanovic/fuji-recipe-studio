param([string]$ToolchainRoot = '')
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
if ($ToolchainRoot) {
    $resolvedTools = (Resolve-Path -LiteralPath $ToolchainRoot).Path
    $env:RUSTUP_HOME = Join-Path $resolvedTools 'rustup'
    $env:CARGO_HOME = Join-Path $resolvedTools 'cargo'
    $env:PATH = (Join-Path $resolvedTools 'cargo\bin') + ';' + (Join-Path $resolvedTools 'mingw64\bin') + ';' + $env:PATH
}
Push-Location $projectRoot
try {
    cargo build --release --locked --bin FujiRecipeStudio
    if ($LASTEXITCODE -ne 0) { throw 'Rust compilation failed.' }
    & (Join-Path $PSScriptRoot 'package.ps1')
} finally { Pop-Location }

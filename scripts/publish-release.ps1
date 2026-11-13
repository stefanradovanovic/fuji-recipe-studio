param(
    [Parameter(Mandatory = $true)][string]$NotesFile,
    [string]$Compiler = 'ISCC.exe',
    [string]$ToolchainRoot = '',
    [string]$SigningThumbprint = '',
    [string]$SignTool = 'signtool.exe'
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$NotesFile = (Resolve-Path -LiteralPath $NotesFile).Path
$repository = 'stefanradovanovic/fuji-recipe-studio'
$previousToken = $env:GH_TOKEN
Push-Location $projectRoot
try {
    if (git status --porcelain) { throw 'Commit the finished changes before publishing a release.' }
    $manifest = Get-Content -LiteralPath 'Cargo.toml' -Raw
    $version = [regex]::Match($manifest, '(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"').Groups[1].Value
    if (-not $version) { throw 'Cargo.toml must contain a stable major.minor.patch version.' }
    $commit = git rev-parse HEAD
    if ($LASTEXITCODE -ne 0) { throw 'Could not read the release commit.' }
    $env:GH_TOKEN = gh auth token --hostname github.com --user stefanradovanovic
    if ($LASTEXITCODE -ne 0) { throw 'Sign in to GitHub CLI as stefanradovanovic first.' }
    $remoteCommit = gh api "repos/$repository/commits/main" --jq .sha
    if ($LASTEXITCODE -ne 0 -or $remoteCommit -ne $commit) { throw 'Push this commit to main before publishing.' }
    & (Join-Path $PSScriptRoot 'build.ps1') -ToolchainRoot $ToolchainRoot
    & (Join-Path $PSScriptRoot 'installer.ps1') -Compiler $Compiler -SigningThumbprint $SigningThumbprint -SignTool $SignTool
    $installer = Join-Path $projectRoot "dist\installer\FujiRecipeStudio-Setup-$version-x64.exe"
    $digest = 'sha256:' + (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
    gh release create "v$version" $installer "$installer.sha256" --repo $repository --target $commit --draft --title "Fuji Recipe Studio $version" --notes-file $NotesFile
    if ($LASTEXITCODE -ne 0) { throw 'Could not create the draft release. Check existing releases before retrying.' }
    # Draft tags are not necessarily addressable through /releases/tags yet.
    $releaseText = gh api "repos/$repository/releases?per_page=100"
    if ($LASTEXITCODE -ne 0) { throw 'Could not verify the draft release. It has not been published.' }
    $releaseList = $releaseText | ConvertFrom-Json
    $matches = @($releaseList | Where-Object { $_.tag_name -eq "v$version" -and $_.draft })
    if ($matches.Count -ne 1 -or $matches[0].target_commitish -ne $commit) { throw 'Could not identify the new release draft. It has not been published.' }
    $release = $matches[0]
    $asset = @($release.assets | Where-Object name -eq ([IO.Path]::GetFileName($installer)))
    if ($asset.Count -ne 1 -or $asset[0].digest -ne $digest) { throw 'Uploaded installer digest mismatch. The release remains a draft.' }
    gh release edit "v$version" --repo $repository --draft=false --latest
    if ($LASTEXITCODE -ne 0) { throw 'Could not publish the verified draft release.' }
    Write-Output "Published https://github.com/$repository/releases/tag/v$version"
} finally {
    $env:GH_TOKEN = $previousToken
    Pop-Location
}

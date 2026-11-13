param([string]$Destination = '')
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
if (-not $Destination) { $Destination = Join-Path $projectRoot 'dist\FujiRecipeStudio' }
$Destination = [IO.Path]::GetFullPath($Destination)
New-Item -ItemType Directory -Force -Path $Destination | Out-Null
Copy-Item -LiteralPath (Join-Path $projectRoot 'target\release\FujiRecipeStudio.exe') -Destination $Destination
foreach ($name in @('README.md', 'LICENSE', 'THIRD_PARTY.md')) {
    Copy-Item -LiteralPath (Join-Path $projectRoot $name) -Destination $Destination
}

Push-Location $projectRoot
try {
    $rustHost = (rustc -vV | Select-String '^host: ').ToString().Substring(6).Trim()
    $metadataText = cargo metadata --locked --format-version 1 --filter-platform $rustHost
    if ($LASTEXITCODE -ne 0) { throw 'Could not read dependency metadata.' }
    $metadata = $metadataText | ConvertFrom-Json
    $resolvedIds = @{}
    foreach ($node in $metadata.resolve.nodes) { $resolvedIds[$node.id] = $true }
    $licenseRoot = Join-Path $Destination 'licenses'
    $sourceRoot = Join-Path $Destination 'source'
    New-Item -ItemType Directory -Force -Path $licenseRoot, $sourceRoot | Out-Null
    $inventory = [Collections.Generic.List[string]]::new()
    $inventory.Add('# Dependency licenses')
    $inventory.Add('')
    $inventory.Add('Resolved Windows dependencies, including build tools. See Cargo.lock for checksums.')
    $inventory.Add('')
    $inventory.Add('| Package | License | Source |')
    $inventory.Add('| --- | --- | --- |')
    foreach ($package in ($metadata.packages | Sort-Object name, version)) {
        if (-not $resolvedIds.ContainsKey($package.id) -or -not $package.source) { continue }
        $packageRoot = Split-Path -Parent $package.manifest_path
        $packageName = "$($package.name)-$($package.version)"
        $inventory.Add("| $packageName | $($package.license) | https://crates.io/crates/$($package.name)/$($package.version) |")
        $licenseFiles = Get-ChildItem -LiteralPath $packageRoot -File -Recurse | Where-Object {
            $_.Name -match '(LICENSE|LICENCE|COPYING|COPYRIGHT|NOTICE|(^|[._-])(OFL|UFL|UNLICENSE)([._-]|$))' -or
            ($package.name -eq 'epaint_default_fonts' -and $_.Extension -eq '.txt')
        }
        foreach ($file in $licenseFiles) {
            $relative = $file.FullName.Substring($packageRoot.Length).TrimStart('\', '/')
            $target = Join-Path (Join-Path $licenseRoot $packageName) $relative
            New-Item -ItemType Directory -Force -Path (Split-Path -Parent $target) | Out-Null
            Copy-Item -LiteralPath $file.FullName -Destination $target
        }
        if ($package.license -match 'LGPL|MPL') {
            $registryName = Split-Path -Leaf (Split-Path -Parent $packageRoot)
            $registryRoot = Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $packageRoot))
            $archive = Join-Path $registryRoot "cache\$registryName\$packageName.crate"
            if (-not (Test-Path -LiteralPath $archive)) { throw "Dependency source archive missing: $archive" }
            Copy-Item -LiteralPath $archive -Destination $sourceRoot
        }
    }
    [IO.File]::WriteAllLines((Join-Path $licenseRoot 'DEPENDENCIES.md'), $inventory, [Text.UTF8Encoding]::new($false))
    Copy-Item -LiteralPath (Join-Path $projectRoot 'notices') -Destination $licenseRoot -Recurse -Force
    $sourceFiles = @('Cargo.toml', 'Cargo.lock', '.gitignore', 'README.md', 'LICENSE', 'THIRD_PARTY.md', 'src', 'scripts', 'examples', 'notices')
    if (Test-Path -LiteralPath 'docs') { $sourceFiles += 'docs' }
    Compress-Archive -LiteralPath $sourceFiles -DestinationPath (Join-Path $sourceRoot 'fuji-recipe-studio-source.zip') -Force
    Write-Output "Portable app and rebuildable source: $Destination"
} finally { Pop-Location }

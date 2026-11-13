param(
    [string]$Compiler = 'ISCC.exe',
    [string]$PackageDir = '',
    [string]$OutputDir = '',
    [string]$Version = ''
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
if (-not $Version) {
    $manifest = Get-Content -LiteralPath (Join-Path $projectRoot 'Cargo.toml') -Raw
    $Version = [regex]::Match($manifest, '(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"').Groups[1].Value
}
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw 'A stable major.minor.patch version is required.' }
if (-not $PackageDir) { $PackageDir = Join-Path $projectRoot 'dist\FujiRecipeStudio' }
if (-not $OutputDir) { $OutputDir = Join-Path $projectRoot 'dist\installer' }
$PackageDir = (Resolve-Path -LiteralPath $PackageDir).Path
$OutputDir = [IO.Path]::GetFullPath($OutputDir)
if (-not (Test-Path -LiteralPath (Join-Path $PackageDir 'FujiRecipeStudio.exe'))) { throw 'Build and package the app first.' }
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
& $Compiler "--define=AppVersion=$Version" "--define=PackageDir=$PackageDir" "--define=OutputPath=$OutputDir" '--quiet' (Join-Path $projectRoot 'installer\setup.iss')
if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed.' }
$installerPath = Join-Path $OutputDir "FujiRecipeStudio-Setup-$Version-x64.exe"
$hash = (Get-FileHash -LiteralPath $installerPath -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText("$installerPath.sha256", "$hash  $([IO.Path]::GetFileName($installerPath))`n", [Text.UTF8Encoding]::new($false))
Write-Output "Installer: $installerPath"

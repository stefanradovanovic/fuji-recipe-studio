param(
    [string]$Compiler = 'ISCC.exe',
    [string]$PackageDir = '',
    [string]$OutputDir = '',
    [string]$Version = '',
    [string]$SigningThumbprint = '',
    [string]$SignTool = 'signtool.exe'
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
$signingArguments = @()
if ($SigningThumbprint) {
    if ($SigningThumbprint -notmatch '\A[0-9a-fA-F]{40}\z') { throw 'Enter the 40-digit code-signing certificate thumbprint.' }
    $certificate = Get-Item -LiteralPath "Cert:\CurrentUser\My\$SigningThumbprint" -ErrorAction SilentlyContinue
    if (-not $certificate -or -not $certificate.HasPrivateKey -or $certificate.NotAfter -le (Get-Date)) {
        throw 'The requested signing certificate and accessible private key must be available in the current user certificate store.'
    }
    $signToolPath = (Get-Command -Name $SignTool -CommandType Application).Source
    $signCommand = '$q' + $signToolPath.Replace('$', '$$') + '$q sign /sha1 ' + $SigningThumbprint + ' /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 $f'
    $signingArguments = @('--define=PublisherSigning', "--signtool=FujiRecipeStudioPublisher=$signCommand")
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
& $Compiler "--define=AppVersion=$Version" "--define=PackageDir=$PackageDir" "--define=OutputPath=$OutputDir" '--quiet' @signingArguments (Join-Path $projectRoot 'installer\setup.iss')
if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed.' }
$installerPath = Join-Path $OutputDir "FujiRecipeStudio-Setup-$Version-x64.exe"
if ($SigningThumbprint) {
    foreach ($file in @($installerPath, (Join-Path $PackageDir 'FujiRecipeStudio.exe'))) {
        $signature = Get-AuthenticodeSignature -LiteralPath $file
        if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Thumbprint -ne $SigningThumbprint -or -not $signature.TimeStamperCertificate) {
            throw "A valid timestamped signature from the requested publisher is missing: $file"
        }
    }
}
$hash = (Get-FileHash -LiteralPath $installerPath -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText("$installerPath.sha256", "$hash  $([IO.Path]::GetFileName($installerPath))`n", [Text.UTF8Encoding]::new($false))
Write-Output "Installer: $installerPath"

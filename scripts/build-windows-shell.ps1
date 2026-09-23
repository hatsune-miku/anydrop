# Build the native extension and sparse identity package. Signing is optional for
# local development, required to distribute Windows 11 modern menu registration.
param([string]$Version = '1.0.0.0')
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$source = Join-Path $root 'apps\desktop-tauri\src-tauri\shell\windows'
$output = Join-Path $root 'apps\desktop-tauri\src-tauri\.generated\shell'
$build = Join-Path $root 'target\windows-shell'
New-Item $output -ItemType Directory -Force | Out-Null
cmake -S $source -B $build -A x64
if ($LASTEXITCODE) { throw 'CMake configuration failed' }
cmake --build $build --config Release
if ($LASTEXITCODE) { throw 'Shell extension compilation failed' }
Copy-Item "$build\Release\AnyDropShell.dll" $output -Force
# Windows PowerShell 5.1 requires a BOM to decode the Chinese menu labels.
$register = Get-Content "$source\register.ps1" -Raw -Encoding UTF8
[IO.File]::WriteAllText("$output\register.ps1", $register, [Text.UTF8Encoding]::new($true))
Add-Type -AssemblyName System.Drawing
$icon = [Drawing.Image]::FromFile((Join-Path $root 'apps\desktop-tauri\src-tauri\icons\128x128.png'))
foreach ($entry in @(@('StoreLogo.png',50), @('Logo.png',150), @('SmallLogo.png',44))) {
    $bitmap = [Drawing.Bitmap]::new($icon, [int]$entry[1], [int]$entry[1])
    $bitmap.Save((Join-Path $output $entry[0]), [Drawing.Imaging.ImageFormat]::Png); $bitmap.Dispose()
}
$icon.Dispose()
$stage = Join-Path $build 'package'
New-Item $stage -ItemType Directory -Force | Out-Null
[xml]$manifest = Get-Content "$source\AppxManifest.xml" -Raw
$manifest.Package.Identity.Version = $Version
$certificateFile = Join-Path $build 'signing.pfx'
$certificate = $null
try {
    if ($env:ANYDROP_WINDOWS_CERTIFICATE) {
        [IO.File]::WriteAllBytes($certificateFile, [Convert]::FromBase64String($env:ANYDROP_WINDOWS_CERTIFICATE))
        $certificate = [Security.Cryptography.X509Certificates.X509Certificate2]::new($certificateFile, $env:ANYDROP_WINDOWS_CERTIFICATE_PASSWORD)
        $manifest.Package.Identity.Publisher = $certificate.Subject
    }
    $manifest.Save("$stage\AppxManifest.xml")
    $sdk = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Directory | Where-Object { Test-Path "$($_.FullName)\x64\makeappx.exe" } | Sort-Object Name -Descending | Select-Object -First 1
    if (!$sdk) { throw 'Windows SDK makeappx.exe not found' }
    $makeappx = "$($sdk.FullName)\x64\makeappx.exe"
    $signtool = "$($sdk.FullName)\x64\signtool.exe"
    # External binaries and assets remain in the NSIS installation, not in MSIX.
    & $makeappx pack /d $stage /p "$output\AnyDrop.FileActions.msix" /nv /o
    if ($LASTEXITCODE) { throw 'Sparse identity packaging failed' }
    if ($certificate) {
        foreach ($file in @("$output\AnyDropShell.dll", "$output\AnyDrop.FileActions.msix")) {
            & $signtool sign /fd SHA256 /f $certificateFile /p $env:ANYDROP_WINDOWS_CERTIFICATE_PASSWORD /tr http://timestamp.digicert.com /td SHA256 $file
            if ($LASTEXITCODE) { throw 'Windows shell signing failed' }
        }
    } else {
        # Keep an unsigned package as a developer artifact, never pass it to users
        # as though Windows could install it without trust/developer provisioning.
        Move-Item "$output\AnyDrop.FileActions.msix" "$build\AnyDrop.FileActions.unsigned.msix" -Force
        Write-Warning 'No Windows signing certificate: classic menus only in this build; modern-menu sources and unsigned test package are available.'
    }
} finally {
    if ($certificate) { $certificate.Dispose() }
    if (Test-Path $certificateFile) { Remove-Item $certificateFile -Force }
}

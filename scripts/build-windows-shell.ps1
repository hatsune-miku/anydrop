# Build only the classic context-menu extension. Windows 11 package identity
# registration is paused; AppxManifest.xml is retained as inactive source.
param([string]$Version = '1.0.0.0')
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$source = Join-Path $root 'apps\desktop-tauri\src-tauri\shell\windows'
$output = Join-Path $root 'apps\desktop-tauri\src-tauri\.generated\shell'
$build = Join-Path $root 'target\windows-shell'
# Do not bundle a stale modern-menu package/assets from an earlier local build.
if (Test-Path -LiteralPath $output) { Remove-Item -LiteralPath $output -Recurse -Force }
New-Item $output -ItemType Directory -Force | Out-Null
cmake -S $source -B $build -A x64
if ($LASTEXITCODE) { throw 'CMake configuration failed' }
cmake --build $build --config Release
if ($LASTEXITCODE) { throw 'Shell extension compilation failed' }
$dllName = "AnyDropShell-$Version.dll"
Copy-Item "$build\Release\AnyDropShell.dll" "$output\$dllName" -Force
@{ dll = $dllName; version = $Version } | ConvertTo-Json | Set-Content "$output\integration.json" -Encoding UTF8
# Windows PowerShell 5.1 requires a BOM to decode the Chinese menu labels.
$register = Get-Content "$source\register.ps1" -Raw -Encoding UTF8
[IO.File]::WriteAllText("$output\register.ps1", $register, [Text.UTF8Encoding]::new($true))

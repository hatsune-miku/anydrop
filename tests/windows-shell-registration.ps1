# Runs only in an isolated Windows CI account; exercises the generated PS 5.1 script.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$install = Join-Path $root 'apps\desktop-tauri\src-tauri\.generated'
$script = Join-Path $install 'shell\register.ps1'
$exe = Join-Path $root 'target\windows-shell\Release\AnyDropShellTest.exe'
if (Get-ChildItem (Join-Path $install 'shell') -Filter '*.msix') { throw 'Modern-menu package must not be bundled' }
try {
    # Both startup and the repair button must succeed without package identity.
    # Repeat startup registration to cover upgrading/relaunching an installed app.
    foreach ($registration in @('Normal', 'Repair', 'Normal')) {
        & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $script -InstallRoot $install -Executable $exe -Registration $registration
        if ($LASTEXITCODE) { throw "$registration registration failed" }
    }
    foreach ($type in @('*','Directory')) {
        foreach ($name in @('AnyDrop.Send','AnyDrop.CopyPaths')) {
            $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey("Software\Classes\$type\shell\$name")
            if (!$key -or !$key.GetValue('ExplorerCommandHandler') -or !$key.GetValue('MUIVerb')) { throw "Missing verb: $type $name" }
            $expectedTitle = if ($name -eq 'AnyDrop.Send') { '使用 AnyDrop 发送' } else { '复制绝对路径' }
            if ($key.GetValue('MUIVerb') -ne $expectedTitle) { throw "Incorrect menu label: $type $name" }
            $id = $key.GetValue('ExplorerCommandHandler'); $key.Dispose()
            $server = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey("Software\Classes\CLSID\$id\InprocServer32")
            if (!$server -or !(Test-Path -LiteralPath $server.GetValue(''))) { throw "Invalid COM server: $id" }
            $server.Dispose()
        }
    }
} finally {
    & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $script -InstallRoot $install -Mode Unregister
    if ($LASTEXITCODE) { throw 'Unregister failed' }
}
foreach ($type in @('*','Directory')) {
    foreach ($name in @('AnyDrop.Send','AnyDrop.CopyPaths')) {
        if ([Microsoft.Win32.Registry]::CurrentUser.OpenSubKey("Software\Classes\$type\shell\$name")) { throw 'Registration was not removed' }
    }
}
Write-Output 'Windows classic menu registration, COM paths, Unicode labels and unregister passed'

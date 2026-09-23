# Runs only in an isolated Windows CI account; exercises the generated PS 5.1 script.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$install = Join-Path $root 'apps\desktop-tauri\src-tauri\.generated'
$script = Join-Path $install 'shell\register.ps1'
$exe = Join-Path $root 'target\windows-shell\Release\AnyDropShellTest.exe'
try {
    # An unsigned CI build intentionally reports the missing modern identity package
    # after registering classic verbs. Registry checks below verify the partial result.
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $output = & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $script -InstallRoot $install -Executable $exe 2>&1
    $registerExit = $LASTEXITCODE
    $ErrorActionPreference = $previous
    if ($registerExit -ne 0 -and "$output" -notmatch '传统菜单已注册') { throw "$output" }
    foreach ($type in @('*','Directory')) {
        foreach ($name in @('AnyDrop.Send','AnyDrop.CopyPaths')) {
            $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey("Software\Classes\$type\shell\$name")
            if (!$key -or !$key.GetValue('ExplorerCommandHandler') -or !$key.GetValue('MUIVerb')) { throw "Missing verb: $type $name" }
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

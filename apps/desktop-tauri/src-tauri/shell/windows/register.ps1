param([Parameter(Mandatory=$true)][string]$InstallRoot,
      [string]$Executable,
      [ValidateSet('Register','Unregister')][string]$Mode = 'Register')
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$classes = 'Registry::HKEY_CURRENT_USER\Software\Classes'
$commands = @(
    @{ Id = '{2F531697-41BE-4EC5-9402-1BBB96F238A0}'; Key = 'AnyDrop.Send'; Title = '使用 AnyDrop 发送' },
    @{ Id = '{9AB91B94-82DB-4A95-85AD-15EAC26D967B}'; Key = 'AnyDrop.CopyPaths'; Title = '复制绝对路径' }
)
if ($Mode -eq 'Unregister') {
    foreach ($command in $commands) {
        foreach ($type in @('*', 'Directory')) {
            $path = "$classes\$type\shell\$($command.Key)"
            if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Recurse -Force }
        }
        $path = "$classes\CLSID\$($command.Id)"
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Recurse -Force }
    }
    Get-AppxPackage -Name 'AnyDrop.FileActions' | Remove-AppxPackage
    if (Test-Path 'HKCU:\Software\AnyDrop\Shell') { Remove-Item 'HKCU:\Software\AnyDrop\Shell' -Recurse -Force }
} else {
    $dll = Join-Path $InstallRoot 'shell\AnyDropShell.dll'
    if (!(Test-Path -LiteralPath $dll)) { throw "缺少菜单扩展，请重新安装 AnyDrop：$dll" }
    if (!$Executable) { $Executable = Join-Path $InstallRoot 'anydrop-desktop-tauri.exe' }
    if (!(Test-Path -LiteralPath $Executable)) { throw "找不到 AnyDrop：$Executable" }
    New-Item 'HKCU:\Software\AnyDrop\Shell' -Force | Out-Null
    Set-ItemProperty 'HKCU:\Software\AnyDrop\Shell' -Name Executable -Value $Executable
    foreach ($command in $commands) {
        $server = "$classes\CLSID\$($command.Id)\InprocServer32"
        New-Item -Path $server -Force | Out-Null
        Set-Item -LiteralPath $server -Value $dll
        New-ItemProperty -LiteralPath $server -Name ThreadingModel -Value Apartment -Force | Out-Null
        foreach ($type in @('*', 'Directory')) {
            $verb = "$classes\$type\shell\$($command.Key)"
            # Registry provider treats * as a wildcard except with -LiteralPath.
            [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey("Software\Classes\$type\shell\$($command.Key)").Dispose()
            New-ItemProperty -LiteralPath $verb -Name MUIVerb -Value $command.Title -Force | Out-Null
            New-ItemProperty -LiteralPath $verb -Name ExplorerCommandHandler -Value $command.Id -Force | Out-Null
            New-ItemProperty -LiteralPath $verb -Name MultiSelectModel -Value Player -Force | Out-Null
            New-ItemProperty -LiteralPath $verb -Name Icon -Value "`"$Executable`",0" -Force | Out-Null
        }
    }
    # Classic registration above works without package identity. Modern menus are
    # installed through the signed sparse package; never change system menu policy.
    $package = Join-Path $InstallRoot 'shell\AnyDrop.FileActions.msix'
    if ([Environment]::OSVersion.Version.Build -ge 22000) {
        if (!(Test-Path -LiteralPath $package)) { throw '传统菜单已注册；此构建缺少已签名的 Windows 11 菜单身份包。' }
        try { Add-AppxPackage -Path $package -ExternalLocation $InstallRoot -ForceUpdateFromAnyVersion }
        catch { throw "传统菜单已注册；Windows 11 新菜单注册失败：$($_.Exception.Message)" }
    }
}
Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class AnyDropShellNotify { [DllImport("shell32.dll")] public static extern void SHChangeNotify(uint e, uint flags, IntPtr a, IntPtr b); }'
[AnyDropShellNotify]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$GitHubRepository = "ray7086/wocao-hub"
$GitHubReleaseBase = "https://github.com/$GitHubRepository/releases/latest/download"
$GitHubChecksumsUrl = "$GitHubReleaseBase/checksums.txt"
$GiteeRepository = "codeTrees/wocao-hub"
$GiteeReleaseBase = "https://gitee.com/$GiteeRepository/releases/download"
$GiteeChecksumsUrl = "https://gitee.com/$GiteeRepository/releases/download/latest/checksums.txt"

function Get-NativeWindowsArchitecture {
    $architecture = if ($env:WOCAO_HUB_INSTALL_ARCH) {
        $env:WOCAO_HUB_INSTALL_ARCH
    } elseif ($env:PROCESSOR_ARCHITEW6432) {
        $env:PROCESSOR_ARCHITEW6432
    } else {
        $env:PROCESSOR_ARCHITECTURE
    }

    switch ($architecture.ToUpperInvariant()) {
        "ARM64" { return "arm64" }
        "AMD64" { return "x64" }
        "X64" { return "x64" }
        default { throw "Wocao Hub 目前仅支持 Windows x64 和 ARM64，检测到：$architecture" }
    }
}

function Remove-InstallerFile {
    param([string]$Path)

    if ($Path -and (Test-Path -LiteralPath $Path)) {
        Remove-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
    }
}

if ($PSVersionTable.PSVersion.Major -lt 5) {
    throw "安装脚本需要 PowerShell 5.1 或更高版本。"
}

if ([Net.ServicePointManager]::SecurityProtocol -band [Net.SecurityProtocolType]::Tls12) {
    # TLS 1.2 is already enabled.
} else {
    [Net.ServicePointManager]::SecurityProtocol =
        [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
}

$assetArchitecture = Get-NativeWindowsArchitecture
$checksumPattern = "^(?<hash>[0-9a-fA-F]{64})\s+(?<name>Wocao\.Hub_(?<version>[0-9]+\.[0-9]+\.[0-9]+)_${assetArchitecture}-setup\.exe)\s*$"

Write-Host "正在获取 Wocao Hub 最新版本信息……"
try {
    $checksums = Invoke-RestMethod -Uri $GiteeChecksumsUrl -Headers @{ "User-Agent" = "wocao-hub-installer" } -Method Get -TimeoutSec 20
    $metadataSource = "Gitee"
} catch {
    Write-Host "国内版本源暂时不可用，正在切换 GitHub……"
    $checksums = Invoke-RestMethod -Uri $GitHubChecksumsUrl -Headers @{ "User-Agent" = "wocao-hub-installer" } -Method Get -TimeoutSec 20
    $metadataSource = "GitHub"
}
$matchingAssets = @(
    foreach ($line in ($checksums -split "`r?`n")) {
        if ($line -match $checksumPattern) {
            [PSCustomObject]@{
                Hash = $Matches["hash"].ToLowerInvariant()
                Name = $Matches["name"]
                Version = $Matches["version"]
            }
        }
    }
)

if ($matchingAssets.Count -ne 1) {
    throw "没有找到唯一的 Windows $assetArchitecture 安装包。"
}

$asset = $matchingAssets[0]
$expectedHash = $asset.Hash
$downloadUris = @(
    [Uri]("$GiteeReleaseBase/v$($asset.Version)/$($asset.Name)"),
    [Uri]("$GitHubReleaseBase/$($asset.Name)")
)

$downloadDirectory = Join-Path ([IO.Path]::GetTempPath()) "WocaoHubInstaller"
$installerPath = Join-Path $downloadDirectory ([string]$asset.Name)
$partialPath = "$installerPath.part"

New-Item -ItemType Directory -Path $downloadDirectory -Force | Out-Null
Remove-InstallerFile -Path $installerPath
Remove-InstallerFile -Path $partialPath

try {
    Write-Host "正在下载 $($asset.Name)（版本信息来源：$metadataSource）……"
    $downloaded = $false
    foreach ($downloadUri in $downloadUris) {
        try {
            Invoke-WebRequest -Uri $downloadUri.AbsoluteUri -Headers @{ "User-Agent" = "wocao-hub-installer" } -OutFile $partialPath -UseBasicParsing -TimeoutSec 90
            $downloaded = $true
            break
        } catch {
            Remove-InstallerFile -Path $partialPath
            if ($downloadUri.Host -eq "gitee.com") {
                Write-Host "国内安装包下载暂时不可用，正在切换 GitHub……"
            }
        }
    }
    if (-not $downloaded) {
        throw "Gitee 和 GitHub 安装包均下载失败。"
    }

    $actualHash = (Get-FileHash -LiteralPath $partialPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne $expectedHash) {
        throw "安装包 SHA-256 校验失败，已停止安装。"
    }

    Move-Item -LiteralPath $partialPath -Destination $installerPath -Force
    Unblock-File -LiteralPath $installerPath

    Write-Host "下载与校验完成：v$($asset.Version) / Windows $assetArchitecture"

    if ($env:WOCAO_HUB_INSTALL_DRY_RUN -eq "1") {
        Write-Host "Dry-run 验证成功，未启动安装器。"
        return
    }

    Write-Host "正在启动安装器……"
    $process = Start-Process -FilePath $installerPath -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        throw "安装器退出代码：$($process.ExitCode)"
    }

    Write-Host "Wocao Hub 安装完成。"
} finally {
    Remove-InstallerFile -Path $partialPath
    Remove-InstallerFile -Path $installerPath
}

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$GitHubRepository = "Tbthr/dadaapi-codex-install-helper"
$GitHubReleaseBase = "https://github.com/$GitHubRepository/releases/latest/download"
$GitHubChecksumsUrl = "$GitHubReleaseBase/checksums.txt"
$GiteeRepository = "lyq_power/dadaapi-codex-install-helper"
$GiteeReleaseBase = "https://gitee.com/$GiteeRepository/releases/download"
$GiteeChecksumsUrl = "https://gitee.com/$GiteeRepository/releases/download/latest/checksums.txt"
$InstallerUserAgent = "dada-assistant-installer"

function Get-NativeWindowsArchitecture {
    $architecture = if ($env:DADA_ASSISTANT_INSTALL_ARCH) {
        $env:DADA_ASSISTANT_INSTALL_ARCH
    } elseif ($env:PROCESSOR_ARCHITEW6432) {
        $env:PROCESSOR_ARCHITEW6432
    } else {
        $env:PROCESSOR_ARCHITECTURE
    }

    switch ($architecture.ToUpperInvariant()) {
        "ARM64" { return "arm64" }
        "AMD64" { return "x64" }
        "X64" { return "x64" }
        default { throw "哒哒助手目前仅支持 Windows x64 和 ARM64，检测到：$architecture" }
    }
}

function Remove-InstallerFile {
    param([string]$Path)

    if ($Path -and (Test-Path -LiteralPath $Path)) {
        Remove-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
    }
}

function Get-ReleaseAsset {
    param(
        [string]$Checksums,
        [string]$Architecture
    )

    $escapedArchitecture = [regex]::Escape($Architecture)
    $checksumPattern = "^(?<hash>[0-9a-fA-F]{64})\s+(?<name>.+_(?<version>[0-9]+\.[0-9]+\.[0-9]+)_${escapedArchitecture}-setup\.exe)\s*$"
    $matchingAssets = @(
        foreach ($line in ($Checksums -split "`r?`n")) {
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
        throw "没有找到唯一的 Windows $Architecture 安装包校验记录。"
    }

    return $matchingAssets[0]
}

function Get-ChecksumText {
    param([string]$Uri)

    return Invoke-RestMethod -Uri $Uri -Headers @{ "User-Agent" = $InstallerUserAgent } -Method Get -TimeoutSec 20
}

function Get-DownloadUri {
    param(
        [string]$Source,
        [object]$Asset
    )

    if ($Source -eq "Gitee") {
        return [Uri]("$GiteeReleaseBase/v$($Asset.Version)/$($Asset.Name)")
    }
    return [Uri]("$GitHubReleaseBase/$($Asset.Name)")
}

function Download-AndVerify {
    param(
        [Uri]$Uri,
        [string]$ExpectedHash,
        [string]$Destination
    )

    Invoke-WebRequest -Uri $Uri.AbsoluteUri -Headers @{ "User-Agent" = $InstallerUserAgent } -OutFile $Destination -UseBasicParsing -TimeoutSec 90
    $actualHash = (Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne $ExpectedHash) {
        throw "安装包 SHA-256 校验失败，已停止安装。"
    }
}

if ($PSVersionTable.PSVersion.Major -lt 5) {
    throw "安装脚本需要 PowerShell 5.1 或更高版本。"
}

if (-not ([Net.ServicePointManager]::SecurityProtocol -band [Net.SecurityProtocolType]::Tls12)) {
    [Net.ServicePointManager]::SecurityProtocol =
        [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
}

$assetArchitecture = Get-NativeWindowsArchitecture
Write-Host "正在获取哒哒助手 v1.0 最新版本信息……"
try {
    $checksums = Get-ChecksumText -Uri $GiteeChecksumsUrl
    $asset = Get-ReleaseAsset -Checksums $checksums -Architecture $assetArchitecture
    $metadataSource = "Gitee"
} catch {
    Write-Host "国内版本源暂时不可用，正在切换 GitHub……"
    $checksums = Get-ChecksumText -Uri $GitHubChecksumsUrl
    $asset = Get-ReleaseAsset -Checksums $checksums -Architecture $assetArchitecture
    $metadataSource = "GitHub"
}

$downloadDirectory = Join-Path ([IO.Path]::GetTempPath()) "DadaAssistantInstaller"
$installerPath = Join-Path $downloadDirectory ([string]$asset.Name)
$partialPath = "$installerPath.part"

New-Item -ItemType Directory -Path $downloadDirectory -Force | Out-Null
Remove-InstallerFile -Path $installerPath
Remove-InstallerFile -Path $partialPath

try {
    Write-Host "正在下载 $($asset.Name)（版本信息来源：$metadataSource）……"
    try {
        Download-AndVerify -Uri (Get-DownloadUri -Source $metadataSource -Asset $asset) -ExpectedHash $asset.Hash -Destination $partialPath
    } catch {
        Remove-InstallerFile -Path $partialPath
        if ($metadataSource -ne "Gitee") {
            throw
        }

        Write-Host "国内安装包下载暂时不可用，正在切换 GitHub……"
        $checksums = Get-ChecksumText -Uri $GitHubChecksumsUrl
        $asset = Get-ReleaseAsset -Checksums $checksums -Architecture $assetArchitecture
        $metadataSource = "GitHub"
        Download-AndVerify -Uri (Get-DownloadUri -Source $metadataSource -Asset $asset) -ExpectedHash $asset.Hash -Destination $partialPath
    }

    Move-Item -LiteralPath $partialPath -Destination $installerPath -Force
    Unblock-File -LiteralPath $installerPath

    Write-Host "下载与校验完成：v$($asset.Version) / Windows $assetArchitecture"

    if ($env:DADA_ASSISTANT_INSTALL_DRY_RUN -eq "1") {
        Write-Host "Dry-run 验证成功，未启动安装器。"
        return
    }

    Write-Host "正在启动哒哒助手安装器……"
    $process = Start-Process -FilePath $installerPath -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        throw "安装器退出代码：$($process.ExitCode)"
    }

    Write-Host "哒哒助手 v1.0 安装完成。"
} finally {
    Remove-InstallerFile -Path $partialPath
    Remove-InstallerFile -Path $installerPath
}

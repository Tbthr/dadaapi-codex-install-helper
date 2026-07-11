$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$Repository = "ray7086/wocao-hub"
$ReleaseApi = "https://api.github.com/repos/$Repository/releases/latest"
$ExpectedReleasePrefix = "/$Repository/releases/download/"

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
$assetNamePattern = "^Wocao\.Hub_[0-9]+\.[0-9]+\.[0-9]+_${assetArchitecture}-setup\.exe$"
$headers = @{
    Accept = "application/vnd.github+json"
    "User-Agent" = "wocao-hub-installer"
    "X-GitHub-Api-Version" = "2022-11-28"
}

Write-Host "正在获取 Wocao Hub 最新版本信息……"
$release = Invoke-RestMethod -Uri $ReleaseApi -Headers $headers -Method Get
$matchingAssets = @($release.assets | Where-Object { $_.name -match $assetNamePattern })

if ($matchingAssets.Count -ne 1) {
    throw "没有找到唯一的 Windows $assetArchitecture 安装包。"
}

$asset = $matchingAssets[0]
$downloadUri = [Uri]$asset.browser_download_url
if ($downloadUri.Scheme -ne "https" -or
    $downloadUri.Host -ne "github.com" -or
    -not $downloadUri.AbsolutePath.StartsWith($ExpectedReleasePrefix, [StringComparison]::Ordinal)) {
    throw "GitHub 返回了不可信的安装包地址。"
}

$digest = [string]$asset.digest
if ($digest -notmatch "^sha256:([0-9a-fA-F]{64})$") {
    throw "GitHub Release 没有提供有效的 SHA-256 摘要。"
}
$expectedHash = $Matches[1].ToLowerInvariant()

$downloadDirectory = Join-Path ([IO.Path]::GetTempPath()) "WocaoHubInstaller"
$installerPath = Join-Path $downloadDirectory ([string]$asset.name)
$partialPath = "$installerPath.part"

New-Item -ItemType Directory -Path $downloadDirectory -Force | Out-Null
Remove-InstallerFile -Path $installerPath
Remove-InstallerFile -Path $partialPath

try {
    Write-Host "正在下载 $($asset.name)……"
    Invoke-WebRequest -Uri $downloadUri.AbsoluteUri -Headers @{ "User-Agent" = "wocao-hub-installer" } -OutFile $partialPath -UseBasicParsing

    $actualHash = (Get-FileHash -LiteralPath $partialPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne $expectedHash) {
        throw "安装包 SHA-256 校验失败，已停止安装。"
    }

    Move-Item -LiteralPath $partialPath -Destination $installerPath -Force
    Unblock-File -LiteralPath $installerPath

    Write-Host "下载与校验完成：$($release.tag_name) / Windows $assetArchitecture"

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

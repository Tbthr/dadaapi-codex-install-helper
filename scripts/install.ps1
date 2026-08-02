$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$GitHubRepository = "Tbthr/dadaapi-codex-install-helper"
$GiteeRepository = "lyq_power/dadaapi-codex-install-helper"
$InstallerUserAgent = "dada-assistant-installer/1.0"
$ExpectedWindowsPublisherSubject = "SET_BEFORE_V1_0_0"
$MaximumChecksumBytes = 65536
$MaximumReleaseMetadataBytes = 1048576
$MaximumInstallerBytes = 1073741824

function Assert-InstallVersion {
    param([Parameter(Mandatory = $true)][string]$Version)

    if ($Version -ne "latest" -and $Version -cnotmatch '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') {
        throw "DADA_ASSISTANT_INSTALL_VERSION 必须为 latest 或 vN.N.N。"
    }
    return $Version
}

function Assert-InstallSource {
    param([Parameter(Mandatory = $true)][string]$Source)

    if ($Source -cnotin @("auto", "gitee", "github")) {
        throw "DADA_ASSISTANT_INSTALL_SOURCE 必须为 auto、gitee 或 github。"
    }
    return $Source
}

function Assert-ReleaseTrustConfiguration {
    if ([string]::IsNullOrWhiteSpace($ExpectedWindowsPublisherSubject) -or
        $ExpectedWindowsPublisherSubject.StartsWith("SET_BEFORE_", [StringComparison]::Ordinal)) {
        throw "安装脚本尚未固化正式 Windows 发布者 Subject，已拒绝运行。"
    }
}

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

function Get-ChecksumsUri {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Version
    )

    if ($Source -eq "github") {
        if ($Version -eq "latest") {
            return [Uri]("https://github.com/$GitHubRepository/releases/latest/download/checksums.txt")
        }
        return [Uri]("https://github.com/$GitHubRepository/releases/download/$Version/checksums.txt")
    }
    if ($Source -eq "gitee") {
        if ($Version -eq "latest") { throw "Gitee latest 必须先解析为不可变标签。" }
        return [Uri]("https://gitee.com/$GiteeRepository/releases/download/$Version/checksums.txt")
    }
    throw "未知安装源：$Source"
}

function Get-GiteeLatestReleaseUri {
    return [Uri]("https://gitee.com/api/v5/repos/$GiteeRepository/releases/latest")
}

function ConvertFrom-GiteeLatestRelease {
    param([Parameter(Mandatory = $true)][string]$Json)

    try {
        $release = $Json | ConvertFrom-Json
    } catch {
        throw "Gitee latest 响应不是有效 JSON。"
    }
    if ($release -is [Array] -or -not $release) {
        throw "Gitee latest 响应结构无效。"
    }
    $properties = @($release.PSObject.Properties.Name)
    if ($properties -cnotcontains "tag_name" -or $release.tag_name -isnot [string] -or
        $properties -cnotcontains "prerelease" -or $release.prerelease -isnot [bool] -or
        $release.prerelease) {
        throw "Gitee latest 必须指向正式 Release。"
    }
    [void](Assert-InstallVersion -Version $release.tag_name)
    if ($release.tag_name -ceq "latest") {
        throw "Gitee latest 返回了无效标签。"
    }
    return [string]$release.tag_name
}

function Get-AssetUri {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][object]$Asset
    )

    if ($Source -eq "github") {
        return [Uri]("https://github.com/$GitHubRepository/releases/download/v$($Asset.Version)/$($Asset.Name)")
    }
    if ($Source -eq "gitee") {
        return [Uri]("https://gitee.com/$GiteeRepository/releases/download/v$($Asset.Version)/$($Asset.Name)")
    }
    throw "未知安装源：$Source"
}

function Assert-HttpsUri {
    param([Parameter(Mandatory = $true)][Uri]$Uri)

    if (-not $Uri.IsAbsoluteUri -or $Uri.Scheme -cne "https" -or $Uri.UserInfo -or [string]::IsNullOrWhiteSpace($Uri.Host)) {
        throw "安装资源必须使用不含凭据的 HTTPS 地址。"
    }
}

function Get-HttpResultKind {
    param(
        [Parameter(Mandatory = $true)][bool]$TransportSucceeded,
        [Parameter(Mandatory = $true)][int]$StatusCode
    )

    if (-not $TransportSucceeded) {
        return "Retryable"
    }
    if ($StatusCode -ge 200 -and $StatusCode -le 299) {
        return "Success"
    }
    if ($StatusCode -ge 500 -and $StatusCode -le 599) {
        return "Retryable"
    }
    return "Fatal"
}

function Test-RetryableTransportException {
    param([Parameter(Mandatory = $true)][Exception]$Exception)

    $current = $Exception
    for ($depth = 0; $current -and $depth -lt 10; $depth++) {
        if ($current -is [OperationCanceledException] -or $current -is [TimeoutException]) {
            return $true
        }
        if ($current -is [Net.WebException]) {
            if ($current.Status -in @(
                [Net.WebExceptionStatus]::ConnectFailure,
                [Net.WebExceptionStatus]::ConnectionClosed,
                [Net.WebExceptionStatus]::KeepAliveFailure,
                [Net.WebExceptionStatus]::NameResolutionFailure,
                [Net.WebExceptionStatus]::PipelineFailure,
                [Net.WebExceptionStatus]::ProxyNameResolutionFailure,
                [Net.WebExceptionStatus]::ReceiveFailure,
                [Net.WebExceptionStatus]::RequestCanceled,
                [Net.WebExceptionStatus]::SendFailure,
                [Net.WebExceptionStatus]::Timeout
            )) {
                return $true
            }
            if ($current.Status -in @(
                [Net.WebExceptionStatus]::ProtocolError,
                [Net.WebExceptionStatus]::SecureChannelFailure,
                [Net.WebExceptionStatus]::TrustFailure
            )) {
                return $false
            }
        }
        if ($current -is [Net.Sockets.SocketException]) {
            return $current.SocketErrorCode -in @(
                [Net.Sockets.SocketError]::ConnectionAborted,
                [Net.Sockets.SocketError]::ConnectionRefused,
                [Net.Sockets.SocketError]::ConnectionReset,
                [Net.Sockets.SocketError]::HostDown,
                [Net.Sockets.SocketError]::HostNotFound,
                [Net.Sockets.SocketError]::HostUnreachable,
                [Net.Sockets.SocketError]::NetworkDown,
                [Net.Sockets.SocketError]::NetworkReset,
                [Net.Sockets.SocketError]::NetworkUnreachable,
                [Net.Sockets.SocketError]::NoData,
                [Net.Sockets.SocketError]::NotConnected,
                [Net.Sockets.SocketError]::TimedOut,
                [Net.Sockets.SocketError]::TryAgain
            )
        }
        if ($current.GetType().FullName -eq "System.Net.Http.HttpRequestException") {
            $requestError = $current.PSObject.Properties["HttpRequestError"]
            if ($requestError -and ([string]$requestError.Value) -in @(
                "NameResolutionError",
                "ConnectionError",
                "ResponseEnded"
            )) {
                return $true
            }
        }
        $current = $current.InnerException
    }
    return $false
}

function Test-ShouldFallbackToGitHub {
    param(
        [Parameter(Mandatory = $true)][string]$InstallSource,
        [Parameter(Mandatory = $true)][string]$CurrentSource,
        [Parameter(Mandatory = $true)][string]$ResultKind
    )

    return $InstallSource -ceq "auto" -and $CurrentSource -ceq "gitee" -and $ResultKind -ceq "Retryable"
}

function Test-WindowsPublisherSubject {
    param(
        [Parameter(Mandatory = $true)][string]$Actual,
        [Parameter(Mandatory = $true)][string]$Expected
    )

    return $Actual -ceq $Expected
}

function Get-HttpsResource {
    param(
        [Parameter(Mandatory = $true)][Uri]$Uri,
        [Parameter(Mandatory = $true)][string]$Destination,
        [Parameter(Mandatory = $true)][long]$MaximumBytes,
        [Parameter(Mandatory = $true)][int]$TimeoutSeconds
    )

    Assert-HttpsUri -Uri $Uri
    $partialPath = "$Destination.part"
    Remove-Item -LiteralPath $partialPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $Destination -Force -ErrorAction SilentlyContinue

    $handler = [Net.Http.HttpClientHandler]::new()
    $handler.AllowAutoRedirect = $false
    $handler.AutomaticDecompression = [Net.DecompressionMethods]::GZip -bor [Net.DecompressionMethods]::Deflate
    $client = [Net.Http.HttpClient]::new($handler)
    $client.Timeout = [Threading.Timeout]::InfiniteTimeSpan
    [void]$client.DefaultRequestHeaders.TryAddWithoutValidation("User-Agent", $InstallerUserAgent)
    $cancellation = [Threading.CancellationTokenSource]::new()
    $cancellation.CancelAfter([TimeSpan]::FromSeconds($TimeoutSeconds))
    $currentUri = $Uri

    try {
        for ($redirectCount = 0; $redirectCount -le 5; $redirectCount++) {
            $response = $null
            try {
                $response = $client.GetAsync(
                    $currentUri,
                    [Net.Http.HttpCompletionOption]::ResponseHeadersRead,
                    $cancellation.Token
                ).GetAwaiter().GetResult()
            } catch {
                Remove-Item -LiteralPath $partialPath -Force -ErrorAction SilentlyContinue
                $kind = if (Test-RetryableTransportException -Exception $_.Exception) { "Retryable" } else { "Fatal" }
                return [PSCustomObject]@{ Kind = $kind; StatusCode = 0; Message = $_.Exception.Message }
            }

            try {
                $statusCode = [int]$response.StatusCode
                if ($statusCode -in @(301, 302, 303, 307, 308)) {
                    if ($redirectCount -eq 5 -or -not $response.Headers.Location) {
                        return [PSCustomObject]@{ Kind = "Fatal"; StatusCode = $statusCode; Message = "重定向无效或超过 5 次。" }
                    }
                    $nextUri = if ($response.Headers.Location.IsAbsoluteUri) {
                        $response.Headers.Location
                    } else {
                        [Uri]::new($currentUri, $response.Headers.Location)
                    }
                    try {
                        Assert-HttpsUri -Uri $nextUri
                    } catch {
                        return [PSCustomObject]@{ Kind = "Fatal"; StatusCode = $statusCode; Message = $_.Exception.Message }
                    }
                    $currentUri = $nextUri
                    continue
                }

                $kind = Get-HttpResultKind -TransportSucceeded $true -StatusCode $statusCode
                if ($kind -ne "Success") {
                    return [PSCustomObject]@{ Kind = $kind; StatusCode = $statusCode; Message = "HTTP $statusCode" }
                }

                if ($response.Content.Headers.ContentLength -and $response.Content.Headers.ContentLength.Value -gt $MaximumBytes) {
                    return [PSCustomObject]@{ Kind = "Fatal"; StatusCode = $statusCode; Message = "响应超过大小限制。" }
                }

                $inputStream = $null
                $outputStream = $null
                try {
                    $inputStream = $response.Content.ReadAsStreamAsync().GetAwaiter().GetResult()
                    $outputStream = [IO.File]::Open($partialPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
                    $buffer = [byte[]]::new(65536)
                    [long]$totalBytes = 0
                    while (($read = $inputStream.ReadAsync(
                        $buffer,
                        0,
                        $buffer.Length,
                        $cancellation.Token
                    ).GetAwaiter().GetResult()) -gt 0) {
                        $totalBytes += $read
                        if ($totalBytes -gt $MaximumBytes) {
                            throw [IO.InvalidDataException]::new("响应超过大小限制。")
                        }
                        $outputStream.Write($buffer, 0, $read)
                    }
                } catch [IO.InvalidDataException] {
                    Remove-Item -LiteralPath $partialPath -Force -ErrorAction SilentlyContinue
                    return [PSCustomObject]@{ Kind = "Fatal"; StatusCode = $statusCode; Message = $_.Exception.Message }
                } catch {
                    Remove-Item -LiteralPath $partialPath -Force -ErrorAction SilentlyContinue
                    $kind = if (Test-RetryableTransportException -Exception $_.Exception) { "Retryable" } else { "Fatal" }
                    return [PSCustomObject]@{ Kind = $kind; StatusCode = 0; Message = $_.Exception.Message }
                } finally {
                    if ($outputStream) { $outputStream.Dispose() }
                    if ($inputStream) { $inputStream.Dispose() }
                }

                Move-Item -LiteralPath $partialPath -Destination $Destination
                return [PSCustomObject]@{ Kind = "Success"; StatusCode = $statusCode; Message = "" }
            } finally {
                $response.Dispose()
            }
        }
    } finally {
        $cancellation.Dispose()
        $client.Dispose()
        $handler.Dispose()
    }

    return [PSCustomObject]@{ Kind = "Fatal"; StatusCode = 0; Message = "重定向处理失败。" }
}

function Get-ReleaseAsset {
    param(
        [Parameter(Mandatory = $true)][string]$Checksums,
        [Parameter(Mandatory = $true)][string]$Architecture,
        [Parameter(Mandatory = $true)][string]$RequestedVersion
    )

    $normalized = $Checksums.TrimEnd([char[]]"`r`n")
    $lines = @($normalized -split "`r?`n")
    if ($lines.Count -ne 3) {
        throw "checksums.txt 必须恰好包含三条 SHA-256 记录。"
    }

    $records = @()
    foreach ($line in $lines) {
        if ($line -cnotmatch '^(?<hash>[0-9a-fA-F]{64})[ \t]+(?<name>\S+)$') {
            throw "checksums.txt 包含无效记录。"
        }
        $name = $Matches["name"]
        if ($name -match '[/\\]' -or $name.StartsWith("-") -or $name.StartsWith(".")) {
            throw "checksums.txt 包含不安全的资产名称。"
        }

        $kind = $null
        $version = $null
        if ($name -cmatch '^Dada-Assistant_(?<version>[0-9]+\.[0-9]+\.[0-9]+)_x64-setup\.exe$') {
            $kind = "x64"
            $version = $Matches["version"]
        } elseif ($name -cmatch '^Dada-Assistant_(?<version>[0-9]+\.[0-9]+\.[0-9]+)_arm64-setup\.exe$') {
            $kind = "arm64"
            $version = $Matches["version"]
        } elseif ($name -cmatch '^Dada-Assistant_(?<version>[0-9]+\.[0-9]+\.[0-9]+)_universal\.dmg$') {
            $kind = "macos"
            $version = $Matches["version"]
        } else {
            throw "checksums.txt 包含发布契约之外的资产。"
        }

        [void](Assert-InstallVersion -Version "v$version")
        $records += [PSCustomObject]@{
            Hash = ($line.Substring(0, 64)).ToLowerInvariant()
            Name = $name
            Version = $version
            Kind = $kind
        }
    }

    foreach ($requiredKind in @("x64", "arm64", "macos")) {
        if (@($records | Where-Object Kind -CEQ $requiredKind).Count -ne 1) {
            throw "checksums.txt 必须为每个平台恰好提供一个资产。"
        }
    }
    $versions = @($records.Version | Sort-Object -Unique)
    if ($versions.Count -ne 1) {
        throw "checksums.txt 中的资产版本不一致。"
    }
    if ($RequestedVersion -ne "latest" -and $RequestedVersion -cne "v$($versions[0])") {
        throw "checksums.txt 中的版本与请求版本不一致。"
    }

    return @($records | Where-Object Kind -CEQ $Architecture)[0]
}

function Test-FileSha256 {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$ExpectedHash
    )

    $actualHash = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    return $actualHash -ceq $ExpectedHash.ToLowerInvariant()
}

function Get-CertificateEkus {
    param([Parameter(Mandatory = $true)][System.Security.Cryptography.X509Certificates.X509Certificate2]$Certificate)

    $oids = @()
    foreach ($extension in $Certificate.Extensions) {
        if ($extension -is [System.Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension]) {
            foreach ($oid in $extension.EnhancedKeyUsages) {
                $oids += $oid.Value
            }
        }
    }
    return $oids
}

function Assert-WindowsInstallerSignature {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$ExpectedPublisherSubject
    )

    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid -or -not $signature.SignerCertificate) {
        throw "安装器 Authenticode 签名无效：$($signature.Status)"
    }
    if (-not (Test-WindowsPublisherSubject -Actual $signature.SignerCertificate.Subject -Expected $ExpectedPublisherSubject)) {
        throw "安装器发布者与固定信任标识不一致。"
    }
    $codeSigningOid = "1.3.6.1.5.5.7.3.3"
    if ((Get-CertificateEkus -Certificate $signature.SignerCertificate) -cnotcontains $codeSigningOid) {
        throw "安装器证书不包含代码签名 EKU。"
    }

    if (-not $signature.TimeStamperCertificate) {
        throw "安装器缺少可信时间戳。"
    }
    $timestampingOid = "1.3.6.1.5.5.7.3.8"
    if ((Get-CertificateEkus -Certificate $signature.TimeStamperCertificate) -cnotcontains $timestampingOid) {
        throw "安装器时间戳证书不包含时间戳 EKU。"
    }

    $chain = [System.Security.Cryptography.X509Certificates.X509Chain]::new()
    try {
        $chain.ChainPolicy.RevocationMode = [System.Security.Cryptography.X509Certificates.X509RevocationMode]::Online
        $chain.ChainPolicy.RevocationFlag = [System.Security.Cryptography.X509Certificates.X509RevocationFlag]::EntireChain
        $chain.ChainPolicy.VerificationFlags = [System.Security.Cryptography.X509Certificates.X509VerificationFlags]::IgnoreNotTimeValid
        if (-not $chain.Build($signature.TimeStamperCertificate)) {
            throw "安装器时间戳证书链不受信任。"
        }
    } finally {
        $chain.Dispose()
    }
}

function Get-ReleaseFromSource {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$RequestedVersion,
        [Parameter(Mandatory = $true)][string]$Architecture,
        [Parameter(Mandatory = $true)][string]$WorkingDirectory
    )

    $resolvedVersion = $RequestedVersion
    if ($Source -ceq "gitee" -and $RequestedVersion -ceq "latest") {
        $releaseMetadataPath = Join-Path $WorkingDirectory "gitee-latest-release.json"
        $releaseMetadataResult = Get-HttpsResource -Uri (Get-GiteeLatestReleaseUri) `
            -Destination $releaseMetadataPath -MaximumBytes $MaximumReleaseMetadataBytes -TimeoutSeconds 30
        if ($releaseMetadataResult.Kind -ne "Success") {
            return [PSCustomObject]@{ Kind = $releaseMetadataResult.Kind; Stage = "release-metadata"; Message = $releaseMetadataResult.Message; Asset = $null; Path = $null; Source = $Source; ResolvedVersion = $null }
        }
        try {
            $releaseMetadata = [IO.File]::ReadAllText($releaseMetadataPath, [Text.Encoding]::UTF8)
            $resolvedVersion = ConvertFrom-GiteeLatestRelease -Json $releaseMetadata
        } catch {
            return [PSCustomObject]@{ Kind = "Fatal"; Stage = "release-metadata"; Message = $_.Exception.Message; Asset = $null; Path = $null; Source = $Source; ResolvedVersion = $null }
        }
    }

    $checksumsPath = Join-Path $WorkingDirectory "checksums-$Source.txt"
    $metadataResult = Get-HttpsResource -Uri (Get-ChecksumsUri -Source $Source -Version $resolvedVersion) `
        -Destination $checksumsPath -MaximumBytes $MaximumChecksumBytes -TimeoutSeconds 30
    if ($metadataResult.Kind -ne "Success") {
        return [PSCustomObject]@{ Kind = $metadataResult.Kind; Stage = "metadata"; Message = $metadataResult.Message; Asset = $null; Path = $null; Source = $Source; ResolvedVersion = $resolvedVersion }
    }

    try {
        $checksums = [IO.File]::ReadAllText($checksumsPath, [Text.Encoding]::UTF8)
        $asset = Get-ReleaseAsset -Checksums $checksums -Architecture $Architecture -RequestedVersion $resolvedVersion
    } catch {
        return [PSCustomObject]@{ Kind = "Fatal"; Stage = "metadata"; Message = $_.Exception.Message; Asset = $null; Path = $null; Source = $Source; ResolvedVersion = $resolvedVersion }
    }

    $installerPath = Join-Path $WorkingDirectory ([string]$asset.Name)
    $assetResult = Get-HttpsResource -Uri (Get-AssetUri -Source $Source -Asset $asset) `
        -Destination $installerPath -MaximumBytes $MaximumInstallerBytes -TimeoutSeconds 300
    if ($assetResult.Kind -ne "Success") {
        return [PSCustomObject]@{ Kind = $assetResult.Kind; Stage = "asset"; Message = $assetResult.Message; Asset = $asset; Path = $null; Source = $Source; ResolvedVersion = $resolvedVersion }
    }
    if (-not (Test-FileSha256 -Path $installerPath -ExpectedHash $asset.Hash)) {
        Remove-Item -LiteralPath $installerPath -Force -ErrorAction SilentlyContinue
        return [PSCustomObject]@{ Kind = "Fatal"; Stage = "hash"; Message = "安装包 SHA-256 校验失败。"; Asset = $asset; Path = $null; Source = $Source; ResolvedVersion = $resolvedVersion }
    }

    return [PSCustomObject]@{ Kind = "Success"; Stage = "complete"; Message = ""; Asset = $asset; Path = $installerPath; Source = $Source; ResolvedVersion = $resolvedVersion }
}

function Invoke-InstallerMain {
    if ($PSVersionTable.PSVersion.Major -lt 5) {
        throw "安装脚本需要 PowerShell 5.1 或更高版本。"
    }
    Add-Type -AssemblyName System.Net.Http
    if (-not ([Net.ServicePointManager]::SecurityProtocol -band [Net.SecurityProtocolType]::Tls12)) {
        [Net.ServicePointManager]::SecurityProtocol =
            [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    }

    Assert-ReleaseTrustConfiguration
    $installVersion = Assert-InstallVersion -Version $(if ($env:DADA_ASSISTANT_INSTALL_VERSION) { $env:DADA_ASSISTANT_INSTALL_VERSION } else { "latest" })
    $installSource = Assert-InstallSource -Source $(if ($env:DADA_ASSISTANT_INSTALL_SOURCE) { $env:DADA_ASSISTANT_INSTALL_SOURCE } else { "auto" })
    $assetArchitecture = Get-NativeWindowsArchitecture
    $workingDirectory = Join-Path ([IO.Path]::GetTempPath()) "DadaAssistantInstaller-$PID-$([Guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory -Path $workingDirectory | Out-Null

    try {
        Write-Host "正在获取哒哒助手 $installVersion 版本信息……"
        if ($installSource -eq "auto") {
            $release = Get-ReleaseFromSource -Source "gitee" -RequestedVersion $installVersion `
                -Architecture $assetArchitecture -WorkingDirectory $workingDirectory
            if (Test-ShouldFallbackToGitHub -InstallSource $installSource -CurrentSource "gitee" -ResultKind $release.Kind) {
                $fallbackVersion = if ($release.ResolvedVersion) { $release.ResolvedVersion } elseif ($release.Asset) { "v$($release.Asset.Version)" } else { $installVersion }
                Write-Host "Gitee 网络暂时不可用，正在切换 GitHub……"
                $release = Get-ReleaseFromSource -Source "github" -RequestedVersion $fallbackVersion `
                    -Architecture $assetArchitecture -WorkingDirectory $workingDirectory
            } elseif ($release.Kind -ne "Success") {
                throw "Gitee 返回的版本信息、资产或校验结果不符合发布契约，已拒绝回退：$($release.Message)"
            }
        } else {
            $release = Get-ReleaseFromSource -Source $installSource -RequestedVersion $installVersion `
                -Architecture $assetArchitecture -WorkingDirectory $workingDirectory
        }

        if ($release.Kind -ne "Success") {
            throw "安装源不可用或未通过校验：$($release.Message)"
        }

        Write-Host "下载与 SHA-256 校验完成：v$($release.Asset.Version) / Windows $assetArchitecture（来源：$($release.Source)）"
        Assert-WindowsInstallerSignature -Path $release.Path -ExpectedPublisherSubject $ExpectedWindowsPublisherSubject

        if ($env:DADA_ASSISTANT_INSTALL_DRY_RUN -eq "1") {
            Write-Host "Dry-run 验证成功，未启动安装器。"
            return
        }

        Write-Host "正在启动哒哒助手安装器……"
        $process = if ($env:DADA_ASSISTANT_INSTALL_CI_SILENT -eq "1") {
            Start-Process -FilePath $release.Path -ArgumentList "/S" -Wait -PassThru
        } else {
            Start-Process -FilePath $release.Path -Wait -PassThru
        }
        if ($process.ExitCode -ne 0) {
            throw "安装器退出代码：$($process.ExitCode)"
        }
        Write-Host "哒哒助手 v$($release.Asset.Version) 安装完成。"
    } finally {
        Remove-Item -LiteralPath $workingDirectory -Recurse -Force -ErrorAction SilentlyContinue
    }
}

if ($env:DADA_ASSISTANT_INSTALL_LIBRARY_ONLY -eq "1") {
    return
}

Invoke-InstallerMain

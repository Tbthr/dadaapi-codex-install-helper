$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$GitHubRepository = "Tbthr/dadaapi-codex-install-helper"
$GiteeRepository = "lyq_power/dadaapi-codex-install-helper"
$BootstrapUserAgent = "dada-assistant-bootstrap/1.0"
$InstallerScriptTag = "v1.0.1"
$InstallerScriptSha256 = "99ce3a2b09fbbd15523799fc6f9389207cec9632b959ad754b59ce6bc5bd270d"
$MaximumInstallerScriptBytes = 1048576

function Assert-InstallSource {
    param([Parameter(Mandatory = $true)][string]$Source)

    if ($Source -cnotin @("auto", "gitee", "github")) {
        throw "DADA_ASSISTANT_INSTALL_SOURCE 必须为 auto、gitee 或 github。"
    }
    return $Source
}

function Get-InstallerScriptUri {
    param([Parameter(Mandatory = $true)][string]$Source)

    switch ($Source) {
        "gitee" {
            return [Uri]("https://gitee.com/$GiteeRepository/raw/$InstallerScriptTag/scripts/install.ps1")
        }
        "github" {
            return [Uri]("https://raw.githubusercontent.com/$GitHubRepository/$InstallerScriptTag/scripts/install.ps1")
        }
        default {
            throw "未知安装源：$Source"
        }
    }
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
    [void]$client.DefaultRequestHeaders.TryAddWithoutValidation("User-Agent", $BootstrapUserAgent)
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

function Test-FileSha256 {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$ExpectedHash
    )

    $actualHash = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    return $actualHash -ceq $ExpectedHash.ToLowerInvariant()
}

function Get-InstallerScript {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$WorkingDirectory
    )

    $destination = Join-Path $WorkingDirectory "install.ps1"
    $result = Get-HttpsResource -Uri (Get-InstallerScriptUri -Source $Source) `
        -Destination $destination -MaximumBytes $MaximumInstallerScriptBytes -TimeoutSeconds 60
    if ($result.Kind -ne "Success") {
        return [PSCustomObject]@{ Kind = $result.Kind; Source = $Source; Path = $null; Message = $result.Message }
    }
    if (-not (Test-FileSha256 -Path $destination -ExpectedHash $InstallerScriptSha256)) {
        Remove-Item -LiteralPath $destination -Force -ErrorAction SilentlyContinue
        return [PSCustomObject]@{ Kind = "Fatal"; Source = $Source; Path = $null; Message = "安装脚本 SHA-256 校验失败。" }
    }
    return [PSCustomObject]@{ Kind = "Success"; Source = $Source; Path = $destination; Message = "" }
}

function Invoke-InstallerScript {
    param([Parameter(Mandatory = $true)][string]$Path)

    $scriptText = [IO.File]::ReadAllText($Path, [Text.UTF8Encoding]::new($false, $true))
    if ([string]::IsNullOrWhiteSpace($scriptText)) {
        throw "安装脚本为空。"
    }
    & ([ScriptBlock]::Create($scriptText))
}

function Invoke-BootstrapMain {
    if ($PSVersionTable.PSVersion.Major -lt 5) {
        throw "安装脚本需要 PowerShell 5.1 或更高版本。"
    }
    Add-Type -AssemblyName System.Net.Http
    if (-not ([Net.ServicePointManager]::SecurityProtocol -band [Net.SecurityProtocolType]::Tls12)) {
        [Net.ServicePointManager]::SecurityProtocol =
            [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    }

    $installSource = Assert-InstallSource -Source $(if ($env:DADA_ASSISTANT_INSTALL_SOURCE) { $env:DADA_ASSISTANT_INSTALL_SOURCE } else { "auto" })
    $workingDirectory = Join-Path ([IO.Path]::GetTempPath()) "DadaAssistantBootstrap-$PID-$([Guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory -Path $workingDirectory | Out-Null
    $previousSource = [Environment]::GetEnvironmentVariable("DADA_ASSISTANT_INSTALL_SOURCE", "Process")
    $restoreSource = $false

    try {
        Write-Host "正在获取哒哒助手安装脚本……"
        if ($installSource -eq "auto") {
            $installerScript = Get-InstallerScript -Source "gitee" -WorkingDirectory $workingDirectory
            if (Test-ShouldFallbackToGitHub -InstallSource $installSource -CurrentSource "gitee" -ResultKind $installerScript.Kind) {
                Write-Host "Gitee 网络暂时不可用，正在切换 GitHub……"
                $installerScript = Get-InstallerScript -Source "github" -WorkingDirectory $workingDirectory
                $env:DADA_ASSISTANT_INSTALL_SOURCE = "github"
                $restoreSource = $true
            } elseif ($installerScript.Kind -ne "Success") {
                throw "Gitee 安装脚本未通过校验，已拒绝回退：$($installerScript.Message)"
            }
        } else {
            $installerScript = Get-InstallerScript -Source $installSource -WorkingDirectory $workingDirectory
        }

        if ($installerScript.Kind -ne "Success") {
            throw "安装脚本不可用或未通过校验：$($installerScript.Message)"
        }
        Invoke-InstallerScript -Path $installerScript.Path
    } finally {
        if ($restoreSource) {
            [Environment]::SetEnvironmentVariable("DADA_ASSISTANT_INSTALL_SOURCE", $previousSource, "Process")
        }
        Remove-Item -LiteralPath $workingDirectory -Recurse -Force -ErrorAction SilentlyContinue
    }
}

if ($env:DADA_ASSISTANT_BOOTSTRAP_LIBRARY_ONLY -eq "1") {
    return
}

Invoke-BootstrapMain

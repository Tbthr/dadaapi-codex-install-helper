$ErrorActionPreference = "Stop"

$env:DADA_ASSISTANT_INSTALL_LIBRARY_ONLY = "1"
. (Join-Path $PSScriptRoot "..\install.ps1")

$script:TestsRun = 0

function Assert-True {
    param([string]$Description, [scriptblock]$Test)
    $script:TestsRun++
    try {
        $result = & $Test
        if (-not $result) {
            throw "returned false"
        }
    } catch {
        throw "FAIL: $Description ($($_.Exception.Message))"
    }
}

function Assert-Throws {
    param([string]$Description, [scriptblock]$Test)
    $script:TestsRun++
    try {
        & $Test | Out-Null
    } catch {
        return
    }
    throw "FAIL: $Description (did not throw)"
}

function Assert-Equal {
    param([string]$Description, [object]$Expected, [object]$Actual)
    $script:TestsRun++
    if ($Expected -cne $Actual) {
        throw "FAIL: $Description (expected '$Expected', got '$Actual')"
    }
}

$fixtureDirectory = Join-Path ([IO.Path]::GetTempPath()) "DadaInstallerTests-$PID-$([Guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $fixtureDirectory | Out-Null
$hashA = "a" * 64
$hashB = "b" * 64
$hashC = "c" * 64
$validChecksums = @"
$hashA  Dada-Assistant_1.0.0_x64-setup.exe
$hashB  Dada-Assistant_1.0.0_arm64-setup.exe
$hashC  Dada-Assistant_1.0.0_universal.dmg
"@

try {
    Assert-Equal "latest version is accepted" "latest" (Assert-InstallVersion -Version "latest")
    Assert-Equal "pinned semantic version is accepted" "v1.0.0" (Assert-InstallVersion -Version "v1.0.0")
    Assert-Throws "version without v is rejected" { Assert-InstallVersion -Version "1.0.0" }
    Assert-Throws "prerelease version is rejected" { Assert-InstallVersion -Version "v1.0.0-rc.1" }
    Assert-Equal "auto source is accepted" "auto" (Assert-InstallSource -Source "auto")
    Assert-Equal "Gitee source is accepted" "gitee" (Assert-InstallSource -Source "gitee")
    Assert-Equal "GitHub source is accepted" "github" (Assert-InstallSource -Source "github")
    Assert-Throws "unknown source is rejected" { Assert-InstallSource -Source "mirror" }

    $savedArchitecture = $env:DADA_ASSISTANT_INSTALL_ARCH
    try {
        $env:DADA_ASSISTANT_INSTALL_ARCH = "AMD64"
        Assert-Equal "x64 architecture is selected" "x64" (Get-NativeWindowsArchitecture)
        $env:DADA_ASSISTANT_INSTALL_ARCH = "ARM64"
        Assert-Equal "ARM64 architecture is selected" "arm64" (Get-NativeWindowsArchitecture)
        $env:DADA_ASSISTANT_INSTALL_ARCH = "x86"
        Assert-Throws "unsupported architecture is rejected" { Get-NativeWindowsArchitecture }
    } finally {
        $env:DADA_ASSISTANT_INSTALL_ARCH = $savedArchitecture
    }

    Assert-Equal "transport errors may fall back" "Retryable" (Get-HttpResultKind -TransportSucceeded $false -StatusCode 0)
    Assert-Equal "5xx responses may fall back" "Retryable" (Get-HttpResultKind -TransportSucceeded $true -StatusCode 503)
    Assert-Equal "4xx responses do not fall back" "Fatal" (Get-HttpResultKind -TransportSucceeded $true -StatusCode 404)
    Assert-Equal "2xx responses continue" "Success" (Get-HttpResultKind -TransportSucceeded $true -StatusCode 200)
    Assert-True "timeouts are retryable transport failures" {
        Test-RetryableTransportException -Exception ([TimeoutException]::new("timeout"))
    }
    Assert-True "DNS failures are retryable transport failures" {
        Test-RetryableTransportException -Exception ([Net.WebException]::new(
            "dns",
            [Net.WebExceptionStatus]::NameResolutionFailure
        ))
    }
    Assert-True "TLS trust failures are fatal transport failures" {
        -not (Test-RetryableTransportException -Exception ([Net.WebException]::new(
            "tls",
            [Net.WebExceptionStatus]::TrustFailure
        )))
    }
    Assert-True "unknown local failures are fatal" {
        -not (Test-RetryableTransportException -Exception ([IO.IOException]::new("disk")))
    }
    Assert-True "auto mode falls back after Gitee transport failure" {
        Test-ShouldFallbackToGitHub -InstallSource "auto" -CurrentSource "gitee" -ResultKind "Retryable"
    }
    Assert-True "auto mode does not fall back after Gitee policy failure" {
        -not (Test-ShouldFallbackToGitHub -InstallSource "auto" -CurrentSource "gitee" -ResultKind "Fatal")
    }
    Assert-True "explicit Gitee mode never falls back" {
        -not (Test-ShouldFallbackToGitHub -InstallSource "gitee" -CurrentSource "gitee" -ResultKind "Retryable")
    }
    Assert-True "GitHub failures never recurse" {
        -not (Test-ShouldFallbackToGitHub -InstallSource "auto" -CurrentSource "github" -ResultKind "Retryable")
    }
    Assert-Equal "Gitee latest uses the release API" `
        "https://gitee.com/api/v5/repos/lyq_power/dadaapi-codex-install-helper/releases/latest" `
        (Get-GiteeLatestReleaseUri).AbsoluteUri
    Assert-Throws "Gitee latest is never treated as a download tag" {
        Get-ChecksumsUri -Source "gitee" -Version "latest"
    }
    Assert-True "matching publisher Subject is accepted" {
        Test-WindowsPublisherSubject -Actual "CN=Dada API" -Expected "CN=Dada API"
    }
    Assert-True "wrong publisher Subject is rejected" {
        -not (Test-WindowsPublisherSubject -Actual "CN=Other" -Expected "CN=Dada API")
    }

    $asset = Get-ReleaseAsset -Checksums $validChecksums -Architecture "x64" -RequestedVersion "v1.0.0"
    Assert-Equal "x64 asset is selected" "Dada-Assistant_1.0.0_x64-setup.exe" $asset.Name
    Assert-Equal "asset version is selected" "1.0.0" $asset.Version
    Assert-Throws "a different pinned version is rejected" {
        Get-ReleaseAsset -Checksums $validChecksums -Architecture "x64" -RequestedVersion "v1.0.1"
    }

$duplicateChecksums = @"
$hashA  Dada-Assistant_1.0.0_x64-setup.exe
$hashB  Dada-Assistant_1.0.0_x64-setup.exe
$hashC  Dada-Assistant_1.0.0_universal.dmg
"@
    Assert-Throws "duplicate assets are rejected" {
        Get-ReleaseAsset -Checksums $duplicateChecksums -Architecture "x64" -RequestedVersion "latest"
    }
$malformedChecksums = @"
not-a-hash  Dada-Assistant_1.0.0_x64-setup.exe
$hashB  Dada-Assistant_1.0.0_arm64-setup.exe
$hashC  Dada-Assistant_1.0.0_universal.dmg
"@
    Assert-Throws "malformed hashes are rejected" {
        Get-ReleaseAsset -Checksums $malformedChecksums -Architecture "x64" -RequestedVersion "latest"
    }
    $wrongNameChecksums = @"
$hashA  Other_1.0.0_x64-setup.exe
$hashB  Dada-Assistant_1.0.0_arm64-setup.exe
$hashC  Dada-Assistant_1.0.0_universal.dmg
"@
    Assert-Throws "unexpected asset prefixes are rejected" {
        Get-ReleaseAsset -Checksums $wrongNameChecksums -Architecture "x64" -RequestedVersion "latest"
    }
    Assert-Equal "Gitee latest final version is parsed" "v1.0.0" `
        (ConvertFrom-GiteeLatestRelease -Json '{"tag_name":"v1.0.0","prerelease":false}')
    Assert-Throws "Gitee prereleases are rejected as latest" {
        ConvertFrom-GiteeLatestRelease -Json '{"tag_name":"v1.0.0","prerelease":true}'
    }
    Assert-Throws "invalid Gitee latest tags are rejected" {
        ConvertFrom-GiteeLatestRelease -Json '{"tag_name":"release-1","prerelease":false}'
    }

    $payloadPath = Join-Path $fixtureDirectory "payload.exe"
    [IO.File]::WriteAllText($payloadPath, "unsigned payload")
    $payloadHash = (Get-FileHash -LiteralPath $payloadPath -Algorithm SHA256).Hash
    Assert-True "matching SHA-256 is accepted" { Test-FileSha256 -Path $payloadPath -ExpectedHash $payloadHash }
    Assert-True "incorrect SHA-256 is rejected" { -not (Test-FileSha256 -Path $payloadPath -ExpectedHash $hashA) }
    Assert-Throws "unsigned installer is rejected" {
        Assert-WindowsInstallerSignature -Path $payloadPath -ExpectedPublisherSubject "CN=Test Publisher"
    }
    $savedPublisher = $ExpectedWindowsPublisherSubject
    try {
        $ExpectedWindowsPublisherSubject = "SET_BEFORE_V1_0_0"
        Assert-Throws "placeholder Windows trust identity is rejected" { Assert-ReleaseTrustConfiguration }
    } finally {
        $ExpectedWindowsPublisherSubject = $savedPublisher
    }

    Assert-Equal "pinned GitHub checksum URL is immutable" `
        "https://github.com/Tbthr/dadaapi-codex-install-helper/releases/download/v1.0.0/checksums.txt" `
        (Get-ChecksumsUri -Source "github" -Version "v1.0.0").AbsoluteUri
    Assert-Equal "pinned Gitee checksum URL is immutable" `
        "https://gitee.com/lyq_power/dadaapi-codex-install-helper/releases/download/v1.0.0/checksums.txt" `
        (Get-ChecksumsUri -Source "gitee" -Version "v1.0.0").AbsoluteUri

    Write-Host "install.ps1 tests passed: $script:TestsRun"
} finally {
    Remove-Item -LiteralPath $fixtureDirectory -Recurse -Force -ErrorAction SilentlyContinue
}

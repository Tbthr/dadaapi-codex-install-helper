$ErrorActionPreference = "Stop"

$previousLibraryOnly = $env:DADA_ASSISTANT_BOOTSTRAP_LIBRARY_ONLY
$env:DADA_ASSISTANT_BOOTSTRAP_LIBRARY_ONLY = "1"
. (Join-Path $PSScriptRoot "..\bootstrap.ps1")

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

try {
    Assert-Equal "bootstrap installer tag is pinned" "v1.0.1" $InstallerScriptTag
    Assert-Equal "auto source is accepted" "auto" (Assert-InstallSource -Source "auto")
    Assert-Equal "Gitee source is accepted" "gitee" (Assert-InstallSource -Source "gitee")
    Assert-Equal "GitHub source is accepted" "github" (Assert-InstallSource -Source "github")
    Assert-Throws "unknown source is rejected" { Assert-InstallSource -Source "mirror" }
    Assert-Equal "Gitee script URL is immutable" `
        "https://gitee.com/lyq_power/dadaapi-codex-install-helper/raw/v1.0.1/scripts/install.ps1" `
        (Get-InstallerScriptUri -Source "gitee").AbsoluteUri
    Assert-Equal "GitHub script URL is immutable" `
        "https://raw.githubusercontent.com/Tbthr/dadaapi-codex-install-helper/v1.0.1/scripts/install.ps1" `
        (Get-InstallerScriptUri -Source "github").AbsoluteUri
    Assert-Throws "non-HTTPS installer URLs are rejected" {
        Assert-HttpsUri -Uri ([Uri]"http://example.test/install.ps1")
    }
    Assert-Equal "transport errors may fall back" "Retryable" (Get-HttpResultKind -TransportSucceeded $false -StatusCode 0)
    Assert-Equal "5xx responses may fall back" "Retryable" (Get-HttpResultKind -TransportSucceeded $true -StatusCode 503)
    Assert-Equal "4xx responses do not fall back" "Fatal" (Get-HttpResultKind -TransportSucceeded $true -StatusCode 404)
    Assert-Equal "2xx responses continue" "Success" (Get-HttpResultKind -TransportSucceeded $true -StatusCode 200)
    Assert-True "auto mode falls back after Gitee transport failure" {
        Test-ShouldFallbackToGitHub -InstallSource "auto" -CurrentSource "gitee" -ResultKind "Retryable"
    }
    Assert-True "auto mode rejects Gitee policy failures" {
        -not (Test-ShouldFallbackToGitHub -InstallSource "auto" -CurrentSource "gitee" -ResultKind "Fatal")
    }
    Assert-True "explicit Gitee mode never falls back" {
        -not (Test-ShouldFallbackToGitHub -InstallSource "gitee" -CurrentSource "gitee" -ResultKind "Retryable")
    }
    Assert-True "installer script hash matches the checked-in source" {
        Test-FileSha256 -Path (Join-Path $PSScriptRoot "..\install.ps1") -ExpectedHash $InstallerScriptSha256
    }

    Write-Host "bootstrap.ps1 tests passed: $script:TestsRun"
} finally {
    [Environment]::SetEnvironmentVariable("DADA_ASSISTANT_BOOTSTRAP_LIBRARY_ONLY", $previousLibraryOnly, "Process")
}

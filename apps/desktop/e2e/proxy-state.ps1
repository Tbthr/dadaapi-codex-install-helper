[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [ValidateSet("read", "save", "restore", "set-baseline")]
  [string]$Action,
  [string]$StatePath
)

$ErrorActionPreference = "Stop"
$registryPath = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings"

function Get-RegistryProperty([string]$name) {
  $item = Get-ItemProperty -Path $registryPath -ErrorAction Stop
  $property = $item.PSObject.Properties[$name]
  return $property
}

function Get-DwordState([string]$name) {
  $property = Get-RegistryProperty $name
  if ($null -eq $property) {
    return [pscustomobject]@{ exists = $false; value = 0 }
  }
  return [pscustomobject]@{ exists = $true; value = [uint32]$property.Value }
}

function Get-StringState([string]$name) {
  $property = Get-RegistryProperty $name
  if ($null -eq $property) {
    return [pscustomobject]@{ exists = $false; value = "" }
  }
  return [pscustomobject]@{ exists = $true; value = [string]$property.Value }
}

function Get-ProxyState {
  return [pscustomobject]@{
    proxyEnable = Get-DwordState "ProxyEnable"
    proxyServer = Get-StringState "ProxyServer"
    proxyOverride = Get-StringState "ProxyOverride"
    autoConfigUrl = Get-StringState "AutoConfigURL"
    autoDetect = Get-DwordState "AutoDetect"
  }
}

function Set-DwordState([string]$name, $entry) {
  if ([bool]$entry.exists) {
    New-ItemProperty -Path $registryPath -Name $name -PropertyType DWord -Value ([uint32]$entry.value) -Force | Out-Null
  } else {
    Remove-ItemProperty -Path $registryPath -Name $name -ErrorAction SilentlyContinue
  }
}

function Set-StringState([string]$name, $entry) {
  if ([bool]$entry.exists) {
    New-ItemProperty -Path $registryPath -Name $name -PropertyType String -Value ([string]$entry.value) -Force | Out-Null
  } else {
    Remove-ItemProperty -Path $registryPath -Name $name -ErrorAction SilentlyContinue
  }
}

function Set-ProxyState($state) {
  New-Item -Path $registryPath -Force | Out-Null
  Set-DwordState "ProxyEnable" $state.proxyEnable
  Set-StringState "ProxyServer" $state.proxyServer
  Set-StringState "ProxyOverride" $state.proxyOverride
  Set-StringState "AutoConfigURL" $state.autoConfigUrl
  Set-DwordState "AutoDetect" $state.autoDetect
  Refresh-InternetSettings
}

function Assert-ProxyState($expected) {
  $actual = Get-ProxyState
  foreach ($name in @("proxyEnable", "autoDetect")) {
    $actualEntry = $actual.PSObject.Properties[$name].Value
    $expectedEntry = $expected.PSObject.Properties[$name].Value
    if ([bool]$actualEntry.exists -ne [bool]$expectedEntry.exists -or
        [uint32]$actualEntry.value -ne [uint32]$expectedEntry.value) {
      throw "proxy_state_verification_failed:$name:expected=$([bool]$expectedEntry.exists)/$([uint32]$expectedEntry.value):actual=$([bool]$actualEntry.exists)/$([uint32]$actualEntry.value)"
    }
  }
  foreach ($name in @("proxyServer", "proxyOverride", "autoConfigUrl")) {
    $actualEntry = $actual.PSObject.Properties[$name].Value
    $expectedEntry = $expected.PSObject.Properties[$name].Value
    if ([bool]$actualEntry.exists -ne [bool]$expectedEntry.exists -or
        [string]$actualEntry.value -ne [string]$expectedEntry.value) {
      throw "proxy_state_verification_failed:$name"
    }
  }
}

function Refresh-InternetSettings {
  Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class DadaAssistantE2eWinInet {
  [DllImport("wininet.dll", SetLastError = true)]
  public static extern bool InternetSetOption(IntPtr internet, int option, IntPtr buffer, int length);
}
'@
  $changed = [DadaAssistantE2eWinInet]::InternetSetOption([IntPtr]::Zero, 39, [IntPtr]::Zero, 0)
  $refreshed = [DadaAssistantE2eWinInet]::InternetSetOption([IntPtr]::Zero, 37, [IntPtr]::Zero, 0)
  if (-not $changed -or -not $refreshed) {
    throw "wininet_notify_failed"
  }
}

function Require-StatePath {
  if ([string]::IsNullOrWhiteSpace($StatePath)) {
    throw "StatePath is required for this action"
  }
}

switch ($Action) {
  "read" {
    Get-ProxyState | ConvertTo-Json -Depth 4 -Compress
    break
  }
  "save" {
    Require-StatePath
    $parent = Split-Path -Parent $StatePath
    if (-not [string]::IsNullOrWhiteSpace($parent)) {
      New-Item -ItemType Directory -Path $parent -Force | Out-Null
    }
    Get-ProxyState | ConvertTo-Json -Depth 4 | Set-Content -Path $StatePath -Encoding utf8
    break
  }
  "restore" {
    Require-StatePath
    $state = Get-Content -Path $StatePath -Raw | ConvertFrom-Json
    Set-ProxyState $state
    Assert-ProxyState $state
    break
  }
  "set-baseline" {
    $baseline = [pscustomobject]@{
      proxyEnable = [pscustomobject]@{ exists = $true; value = [uint32]1 }
      proxyServer = [pscustomobject]@{ exists = $true; value = "127.0.0.1:65534" }
      proxyOverride = [pscustomobject]@{ exists = $true; value = "<local>;localhost;*.localhost;127.0.0.1;::1" }
      autoConfigUrl = [pscustomobject]@{ exists = $true; value = "https://baseline.invalid/proxy.pac" }
      autoDetect = [pscustomobject]@{ exists = $true; value = [uint32]0 }
    }
    Set-ProxyState $baseline
    Assert-ProxyState $baseline
    break
  }
}

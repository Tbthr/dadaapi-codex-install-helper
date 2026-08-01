[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [ValidateSet("read", "save", "restore", "set-baseline")]
  [string]$Action,
  [string]$StatePath
)

$ErrorActionPreference = "Stop"
$registryPath = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings"

Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;

public static class DadaAssistantE2eWinInet
{
  private const uint InternetOptionRefresh = 37;
  private const uint InternetOptionSettingsChanged = 39;
  private const uint InternetOptionPerConnectionOption = 75;
  private const uint PerConnectionFlags = 1;
  private const uint PerConnectionFlagsUi = 10;

  [StructLayout(LayoutKind.Explicit)]
  private struct OptionValue
  {
    [FieldOffset(0)] public uint DwordValue;
    [FieldOffset(0)] public IntPtr StringValue;
    [FieldOffset(0)] public System.Runtime.InteropServices.ComTypes.FILETIME FileTimeValue;
  }

  [StructLayout(LayoutKind.Sequential)]
  private struct Option
  {
    public uint OptionId;
    public OptionValue Value;
  }

  [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
  private struct OptionList
  {
    public uint Size;
    public IntPtr Connection;
    public uint OptionCount;
    public uint OptionError;
    public IntPtr Options;
  }

  [DllImport("wininet.dll", EntryPoint = "InternetQueryOptionW", SetLastError = true)]
  [return: MarshalAs(UnmanagedType.Bool)]
  private static extern bool InternetQueryOption(
    IntPtr internet,
    uint option,
    IntPtr buffer,
    ref uint bufferLength);

  [DllImport("wininet.dll", EntryPoint = "InternetSetOptionW", SetLastError = true)]
  [return: MarshalAs(UnmanagedType.Bool)]
  private static extern bool InternetSetOption(
    IntPtr internet,
    uint option,
    IntPtr buffer,
    uint bufferLength);

  private static readonly int OptionSize = Marshal.SizeOf(typeof(Option));
  private static readonly int OptionListSize = Marshal.SizeOf(typeof(OptionList));

  private static void CheckLayout()
  {
    if (IntPtr.Size == 8 &&
        (OptionSize != 16 ||
         Marshal.OffsetOf(typeof(Option), "Value").ToInt64() != 8 ||
         OptionListSize != 32 ||
         Marshal.OffsetOf(typeof(OptionList), "Options").ToInt64() != 24))
    {
      throw new InvalidOperationException("unexpected WinINet x64 layout");
    }
  }

  private static bool TryQuery(uint option, out uint flags, out int error)
  {
    CheckLayout();
    IntPtr options = IntPtr.Zero;
    IntPtr listPointer = IntPtr.Zero;
    try
    {
      options = Marshal.AllocHGlobal(OptionSize);
      Marshal.StructureToPtr(new Option { OptionId = option }, options, false);
      listPointer = Marshal.AllocHGlobal(OptionListSize);
      Marshal.StructureToPtr(new OptionList {
        Size = (uint)OptionListSize,
        Connection = IntPtr.Zero,
        OptionCount = 1,
        Options = options
      }, listPointer, false);

      uint bufferLength = (uint)OptionListSize;
      bool succeeded = InternetQueryOption(
        IntPtr.Zero,
        InternetOptionPerConnectionOption,
        listPointer,
        ref bufferLength);
      error = succeeded ? 0 : Marshal.GetLastWin32Error();
      flags = succeeded
        ? ((Option)Marshal.PtrToStructure(options, typeof(Option))).Value.DwordValue
        : 0;
      return succeeded;
    }
    finally
    {
      if (listPointer != IntPtr.Zero) Marshal.FreeHGlobal(listPointer);
      if (options != IntPtr.Zero) Marshal.FreeHGlobal(options);
    }
  }

  public static uint QueryFlags()
  {
    int error;
    uint flags;
    if (TryQuery(PerConnectionFlagsUi, out flags, out error) ||
        TryQuery(PerConnectionFlags, out flags, out error))
    {
      return flags;
    }
    throw new Win32Exception(error, "InternetQueryOptionW flags failed");
  }

  private static bool TrySet(uint option, uint flags, out int error)
  {
    CheckLayout();
    IntPtr options = IntPtr.Zero;
    IntPtr listPointer = IntPtr.Zero;
    try
    {
      options = Marshal.AllocHGlobal(OptionSize);
      Marshal.StructureToPtr(new Option {
        OptionId = option,
        Value = new OptionValue { DwordValue = flags }
      }, options, false);
      listPointer = Marshal.AllocHGlobal(OptionListSize);
      Marshal.StructureToPtr(new OptionList {
        Size = (uint)OptionListSize,
        Connection = IntPtr.Zero,
        OptionCount = 1,
        Options = options
      }, listPointer, false);

      bool succeeded = InternetSetOption(
        IntPtr.Zero,
        InternetOptionPerConnectionOption,
        listPointer,
        (uint)OptionListSize);
      error = succeeded ? 0 : Marshal.GetLastWin32Error();
      return succeeded;
    }
    finally
    {
      if (listPointer != IntPtr.Zero) Marshal.FreeHGlobal(listPointer);
      if (options != IntPtr.Zero) Marshal.FreeHGlobal(options);
    }
  }

  private static void Notify()
  {
    if (!InternetSetOption(IntPtr.Zero, InternetOptionSettingsChanged, IntPtr.Zero, 0))
    {
      throw new Win32Exception(Marshal.GetLastWin32Error());
    }
    if (!InternetSetOption(IntPtr.Zero, InternetOptionRefresh, IntPtr.Zero, 0))
    {
      throw new Win32Exception(Marshal.GetLastWin32Error());
    }
  }

  public static void SetFlags(uint flags)
  {
    int error;
    if (!TrySet(PerConnectionFlags, flags, out error))
    {
      throw new Win32Exception(error, "InternetSetOptionW flags failed");
    }
    Notify();
  }

  public static void RestoreFlags(uint flags)
  {
    int error;
    if (!TrySet(PerConnectionFlags, flags, out error))
    {
      throw new Win32Exception(error, "InternetSetOptionW restore flags failed");
    }
    Notify();
  }

  public const uint ProxyTypeDirect = 0x00000001;
  public const uint ProxyTypeProxy = 0x00000002;
  public const uint ProxyTypeAutoProxyUrl = 0x00000004;
  public const uint ProxyTypeAutoDetect = 0x00000008;
}
'@

function Get-WinInetProxyFlagsState {
  try {
    return [pscustomobject]@{
      exists = $true
      value = [uint32][DadaAssistantE2eWinInet]::QueryFlags()
    }
  } catch {
    throw "per_connection_flags_read_failed"
  }
}

function Set-WinInetProxyFlags([uint32]$flags) {
  try {
    [DadaAssistantE2eWinInet]::SetFlags($flags)
  } catch {
    throw "per_connection_flags_write_failed"
  }
}

function Restore-WinInetProxyFlags([uint32]$flags) {
  try {
    [DadaAssistantE2eWinInet]::RestoreFlags($flags)
  } catch {
    throw "per_connection_flags_write_failed"
  }
}

function Get-RegistryProperty([string]$name) {
  $item = Get-ItemProperty -Path $registryPath -ErrorAction Stop
  return $item.PSObject.Properties[$name]
}

function Get-DwordState([string]$name) {
  $property = Get-RegistryProperty $name
  if ($null -eq $property) {
    return [pscustomobject]@{ exists = $false; value = [uint32]0 }
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
    perConnectionFlags = Get-WinInetProxyFlagsState
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

function Set-ProxyState($state, [bool]$restore = $false) {
  New-Item -Path $registryPath -Force | Out-Null
  Set-DwordState "ProxyEnable" $state.proxyEnable
  Set-StringState "ProxyServer" $state.proxyServer
  Set-StringState "ProxyOverride" $state.proxyOverride
  Set-StringState "AutoConfigURL" $state.autoConfigUrl
  Set-DwordState "AutoDetect" $state.autoDetect

  $flags = $state.PSObject.Properties["perConnectionFlags"]
  if ($null -eq $flags -or -not [bool]$flags.Value.exists) {
    throw "per_connection_flags_missing"
  }
  if ($restore) {
    Restore-WinInetProxyFlags ([uint32]$flags.Value.value)
  } else {
    Set-WinInetProxyFlags ([uint32]$flags.Value.value)
  }
}

function Assert-ProxyState($expected) {
  $actual = Get-ProxyState
  foreach ($name in @("proxyEnable", "perConnectionFlags")) {
    $actualEntry = $actual.PSObject.Properties[$name].Value
    $expectedEntry = $expected.PSObject.Properties[$name].Value
    if ([bool]$actualEntry.exists -ne [bool]$expectedEntry.exists -or
        [uint32]$actualEntry.value -ne [uint32]$expectedEntry.value) {
      throw "proxy_state_verification_failed:${name}:expected=$([bool]$expectedEntry.exists)/$([uint32]$expectedEntry.value):actual=$([bool]$actualEntry.exists)/$([uint32]$actualEntry.value)"
    }
  }
  $actualAutoDetect = $actual.PSObject.Properties["autoDetect"].Value
  $expectedAutoDetect = $expected.PSObject.Properties["autoDetect"].Value
  # WinINet may normalize an existing legacy value during refresh, but must not
  # introduce the value when the saved state did not contain it.
  if (-not [bool]$expectedAutoDetect.exists -or [bool]$actualAutoDetect.exists) {
    if ([bool]$actualAutoDetect.exists -ne [bool]$expectedAutoDetect.exists -or
        [uint32]$actualAutoDetect.value -ne [uint32]$expectedAutoDetect.value) {
      throw "proxy_state_verification_failed:autoDetect:expected=$([bool]$expectedAutoDetect.exists)/$([uint32]$expectedAutoDetect.value):actual=$([bool]$actualAutoDetect.exists)/$([uint32]$actualAutoDetect.value)"
    }
  }
  foreach ($name in @("proxyServer", "proxyOverride", "autoConfigUrl")) {
    $actualEntry = $actual.PSObject.Properties[$name].Value
    $expectedEntry = $expected.PSObject.Properties[$name].Value
    if ([bool]$actualEntry.exists -ne [bool]$expectedEntry.exists -or
        [string]$actualEntry.value -ne [string]$expectedEntry.value) {
      throw "proxy_state_verification_failed:${name}"
    }
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
    Set-ProxyState $state $true
    Assert-ProxyState $state
    break
  }
  "set-baseline" {
    $baseline = [pscustomobject]@{
      proxyEnable = [pscustomobject]@{ exists = $true; value = [uint32]0 }
      proxyServer = [pscustomobject]@{ exists = $true; value = "127.0.0.1:65534" }
      proxyOverride = [pscustomobject]@{ exists = $true; value = "<local>;localhost;*.localhost;127.0.0.1;::1" }
      autoConfigUrl = [pscustomobject]@{ exists = $true; value = "https://baseline.invalid/proxy.pac" }
      autoDetect = [pscustomobject]@{ exists = $false; value = [uint32]0 }
      perConnectionFlags = [pscustomobject]@{
        exists = $true
        value = [uint32]([DadaAssistantE2eWinInet]::ProxyTypeAutoProxyUrl -bor [DadaAssistantE2eWinInet]::ProxyTypeAutoDetect)
      }
    }
    Set-ProxyState $baseline
    $actualBaseline = Get-ProxyState
    $actualFlags = [uint32]$actualBaseline.perConnectionFlags.value
    $automaticFlags = [DadaAssistantE2eWinInet]::ProxyTypeAutoProxyUrl -bor [DadaAssistantE2eWinInet]::ProxyTypeAutoDetect
    if (($actualFlags -band [DadaAssistantE2eWinInet]::ProxyTypeProxy) -ne 0 -or
        ($actualFlags -band $automaticFlags) -eq 0) {
      throw "proxy_baseline_verification_failed:flags=$actualFlags"
    }
    $baseline.perConnectionFlags.value = $actualFlags
    Assert-ProxyState $baseline
    break
  }
}

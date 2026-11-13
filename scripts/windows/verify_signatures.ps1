<#
Check that the MSI and every PE file it installs carry a valid, timestamped
Authenticode signature from our organisation.

  verify_signatures.ps1 -Msi <path> -Mode report|enforce -Publisher "GAYOSO LLC"

The file list comes from an administrative extract of the MSI itself, not from
the build directory, so it is exactly what a user installs. A sidecar or DLL
added later is checked without anyone remembering to list it here.

-Mode comes from scripts/windows_sign_config.sh:
  report   print the table and warn about unsigned files; exit 0. This is the
           pre-signing behaviour, so the gate can land before signing does.
  enforce  any failed check exits 1.

In both modes an extract that yields no PE file at all fails: a gate that
checked nothing would report success while proving nothing.
#>
param(
    [Parameter(Mandatory)] [string] $Msi,
    [Parameter(Mandatory)] [ValidateSet('report', 'enforce')] [string] $Mode,
    [Parameter(Mandatory)] [string] $Publisher
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Find-SignTool {
    # Not on PATH on the hosted runner. The newest SDK first, since an old
    # signtool can misreport newer signature formats.
    $kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    $tool = Get-ChildItem -Path $kits -Filter signtool.exe -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.Directory.Name -eq 'x64' } |
        Sort-Object { [version]$_.Directory.Parent.Name } -Descending |
        Select-Object -First 1
    if (-not $tool) { throw "signtool.exe not found under $kits" }
    return $tool.FullName
}

function Expand-Msi([string] $MsiPath) {
    $target = Join-Path $env:RUNNER_TEMP 'msi-extract'
    if (Test-Path $target) { Remove-Item -Recurse -Force $target }
    New-Item -ItemType Directory -Path $target | Out-Null
    $log = Join-Path $env:RUNNER_TEMP 'msi-extract.log'

    # /a is an administrative install: it lays out the files the MSI would
    # install without running the product's custom actions or touching the
    # machine's installed-products list.
    $proc = Start-Process msiexec.exe -Wait -PassThru -ArgumentList @(
        '/a', "`"$MsiPath`"", '/qn', "TARGETDIR=`"$target`"", '/l*v', "`"$log`""
    )
    if ($proc.ExitCode -ne 0) {
        Get-Content $log -Tail 40 | Write-Host
        throw "msiexec /a failed with exit code $($proc.ExitCode)"
    }
    return $target
}

function Test-File([string] $SignTool, [string] $Path, [string] $Root) {
    $problems = @()

    # /pa: the default Authenticode policy, the one Windows applies to a
    # downloaded file. Without it signtool checks the driver policy instead.
    $out = & $SignTool verify /pa /v $Path 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) { $problems += 'signtool verify /pa failed' }
    elseif ($out -notmatch 'Hash of file \(sha256\)') { $problems += 'digest is not SHA256' }

    $sig = Get-AuthenticodeSignature -FilePath $Path
    $subject = if ($sig.SignerCertificate) { $sig.SignerCertificate.Subject } else { '' }
    if ($sig.Status -ne 'Valid') { $problems += "status $($sig.Status)" }
    # O= rather than CN=: the organisation is what the CA validated, and it is
    # what UAC shows as the verified publisher.
    $org = [regex]::Escape($Publisher)
    if ($subject -and $subject -notmatch "(^|,\s*)O=`"?$org`"?(,|$)") { $problems += "signed by '$subject'" }
    # Without a timestamp the signature dies with the certificate, and
    # Artifact Signing certificates last days, not years.
    if ($sig.Status -eq 'Valid' -and -not $sig.TimeStamperCertificate) { $problems += 'no timestamp' }

    $name = if ($Root) { $Path.Substring($Root.Length).TrimStart('\') } else { Split-Path $Path -Leaf }
    [pscustomobject]@{
        File   = $name
        Signer = if ($subject) { ($subject -split ',')[0] } else { '(unsigned)' }
        Result = if ($problems) { 'FAIL: ' + ($problems -join '; ') } else { 'ok' }
    }
}

$msiPath = (Resolve-Path $Msi).Path
$signTool = Find-SignTool
Write-Host "signtool: $signTool"
Write-Host "Mode: $Mode, expected publisher: O=$Publisher"

$root = Expand-Msi $msiPath
$pe = @(Get-ChildItem -Path $root -Recurse -File -Include *.exe, *.dll, *.sys)
if ($pe.Count -eq 0) {
    Write-Host "::error::The MSI extract holds no .exe/.dll/.sys, so the gate would check nothing. Extract: $root"
    exit 1
}

$results = @(Test-File $signTool $msiPath $null)
foreach ($file in $pe) { $results += Test-File $signTool $file.FullName $root }
$results | Format-Table -AutoSize -Wrap | Out-String -Width 200 | Write-Host

$failed = @($results | Where-Object { $_.Result -ne 'ok' })
if ($failed.Count -eq 0) {
    Write-Host "All $($results.Count) files are signed by $Publisher, SHA256, timestamped."
    exit 0
}

if ($Mode -eq 'enforce') {
    Write-Host "::error::$($failed.Count) of $($results.Count) Windows files failed the signature check (table above)."
    exit 1
}

Write-Host "::warning::Unsigned Windows build: $($failed.Count) of $($results.Count) files are not signed by $Publisher. Users will see SmartScreen and 'Unknown publisher' warnings."
exit 0

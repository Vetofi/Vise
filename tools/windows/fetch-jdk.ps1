param([Parameter(Mandatory = $true)][string]$Out)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$url = 'https://api.adoptium.net/v3/binary/latest/25/ga/windows/x64/jdk/hotspot/normal/eclipse'
$tmp = Join-Path $env:TEMP 'vise-jdk'
if (Test-Path $tmp) { Remove-Item $tmp -Recurse -Force }
New-Item -ItemType Directory -Path $tmp | Out-Null
$zip = Join-Path $tmp 'jdk.zip'
Invoke-WebRequest $url -OutFile $zip
Expand-Archive $zip -DestinationPath $tmp -Force
$inner = Get-ChildItem $tmp -Directory | Where-Object { Test-Path (Join-Path $_.FullName 'bin\javac.exe') } | Select-Object -First 1
if (-not $inner) { throw 'The downloaded JDK has no bin\javac.exe' }
New-Item -ItemType Directory -Path (Split-Path $Out) -Force | Out-Null
if (Test-Path $Out) { Remove-Item $Out -Recurse -Force }
Move-Item $inner.FullName $Out
Remove-Item $tmp -Recurse -Force

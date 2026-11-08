param([Parameter(Mandatory = $true)][string]$Out)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$lock = Get-Content (Join-Path $root 'research-lock.json') -Raw | ConvertFrom-Json
$version = $lock.minecraft.version
$wantSha1 = $lock.minecraft.server_sha1
$manifest = Invoke-RestMethod 'https://piston-meta.mojang.com/mc/game/version_manifest_v2.json'
$entry = $manifest.versions | Where-Object { $_.id -eq $version } | Select-Object -First 1
if (-not $entry) { throw "Mojang's version list has no $version" }
$meta = Invoke-RestMethod $entry.url
$jarUrl = $meta.downloads.server.url
New-Item -ItemType Directory -Path (Split-Path $Out) -Force | Out-Null
Invoke-WebRequest $jarUrl -OutFile $Out
$sha1 = (Get-FileHash $Out -Algorithm SHA1).Hash.ToLower()
if ($wantSha1 -and $sha1 -ne $wantSha1.ToLower()) {
  Remove-Item $Out -Force
  throw "server.jar checksum mismatch (got $sha1, expected $wantSha1)"
}

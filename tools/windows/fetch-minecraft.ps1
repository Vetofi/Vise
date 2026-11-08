param(
  [Parameter(Mandatory = $true)][string]$Dir,
  [switch]$NoAssets
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$lock = Get-Content (Join-Path $root 'research-lock.json') -Raw | ConvertFrom-Json
$version = $lock.minecraft.version
$wc = New-Object System.Net.WebClient

function Get-Sha1($path) { (Get-FileHash $path -Algorithm SHA1).Hash.ToLower() }

function Fetch($url, $dest, $sha1) {
  if ((Test-Path $dest) -and ((-not $sha1) -or ((Get-Sha1 $dest) -eq $sha1.ToLower()))) { return }
  New-Item -ItemType Directory -Path (Split-Path $dest) -Force | Out-Null
  $tmp = "$dest.part"
  for ($try = 1; $try -le 3; $try++) {
    try {
      $wc.DownloadFile($url, $tmp)
      if ($sha1 -and ((Get-Sha1 $tmp) -ne $sha1.ToLower())) { throw "checksum mismatch for $url" }
      Move-Item $tmp $dest -Force
      return
    } catch {
      if (Test-Path $tmp) { Remove-Item $tmp -Force }
      if ($try -eq 3) { throw }
      Start-Sleep -Seconds 2
    }
  }
}

function Library-Allowed($lib) {
  if (-not $lib.rules) { return $true }
  $allowed = $false
  foreach ($rule in $lib.rules) {
    if ($rule.features) { continue }
    if ($rule.os) {
      if ($rule.os.name -and $rule.os.name -ne 'windows') { continue }
      if ($rule.os.arch -and $rule.os.arch -eq 'x86') { continue }
    }
    $allowed = ($rule.action -eq 'allow')
  }
  return $allowed
}

Write-Host "Downloading Minecraft $version from Mojang..."
$manifest = Invoke-RestMethod 'https://piston-meta.mojang.com/mc/game/version_manifest_v2.json'
$entry = $manifest.versions | Where-Object { $_.id -eq $version } | Select-Object -First 1
if (-not $entry) { throw "Mojang's version list has no $version" }

$versionDir = Join-Path $Dir "versions\$version"
$jsonPath = Join-Path $versionDir "$version.json"
Fetch $entry.url $jsonPath $entry.sha1
$meta = Get-Content $jsonPath -Raw | ConvertFrom-Json

Write-Host "  client jar"
Fetch $meta.downloads.client.url (Join-Path $versionDir "$version.jar") $meta.downloads.client.sha1

$libs = @($meta.libraries | Where-Object { $_.downloads.artifact -and (Library-Allowed $_) })
$i = 0
foreach ($lib in $libs) {
  $i++
  $name = [string]$lib.name
  $classifier = ($name -split ':')[3]
  if ($classifier -match '(arm64|aarch64|aarch_64)$') { continue }
  $art = $lib.downloads.artifact
  Write-Host ("  libraries {0}/{1}" -f $i, $libs.Count)
  Fetch $art.url (Join-Path $Dir ("libraries\" + ($art.path -replace '/', '\'))) $art.sha1
}

if ($NoAssets) { return }

Write-Host "  asset index"
$indexPath = Join-Path $Dir ("assets\indexes\" + $meta.assetIndex.id + ".json")
Fetch $meta.assetIndex.url $indexPath $meta.assetIndex.sha1
$index = Get-Content $indexPath -Raw -Encoding UTF8 | ConvertFrom-Json

$wanted = $index.objects.PSObject.Properties | Where-Object {
  $n = $_.Name
  $n -eq 'pack.mcmeta' -or $n -eq 'minecraft/sounds.json' -or
  $n.StartsWith('minecraft/sounds/') -or $n.StartsWith('minecraft/lang/') -or
  $n.StartsWith('minecraft/font/') -or $n.StartsWith('minecraft/resourcepacks/') -or
  $n.StartsWith('minecraft/textures/gui/title/background/panorama_')
}
$total = @($wanted).Count
$i = 0
foreach ($p in $wanted) {
  $i++
  $hash = $p.Value.hash
  $sub = $hash.Substring(0, 2)
  if (($i % 100) -eq 0 -or $i -eq $total) { Write-Host ("  assets {0}/{1}" -f $i, $total) }
  Fetch "https://resources.download.minecraft.net/$sub/$hash" (Join-Path $Dir "assets\objects\$sub\$hash") $hash
}
Write-Host "Minecraft $version downloaded."

param(
  [Parameter(Mandatory = $true)][string]$PackRoot,
  [string]$ReleaseArchive,
  [string]$QualificationFile
)
$ErrorActionPreference = 'Stop'
$manifestPath = Join-Path $PackRoot 'manifest.json'
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) { throw 'manifest.json is missing' }
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if ($manifest.protocolVersion -ne '2') { throw 'unsupported pack protocol' }
if ($manifest.licenseExpression -match 'GPL|AGPL|NonCommercial') { throw 'restricted license expression' }
if ([System.IO.Path]::IsPathRooted($manifest.entrypoint) -or $manifest.entrypoint -match '(^|[\\/])\.\.([\\/]|$)') {
  throw 'entrypoint escapes pack root'
}
if ($manifest.packId -eq 'document-standard') {
  $lock = Get-Content -LiteralPath (Join-Path $PackRoot 'requirements.lock') -Raw
  if ($lock -match '\[all\]' -or $lock -match '(?im)^.*pdf.*$') { throw 'document-standard includes forbidden extras' }
  if ($lock -notmatch 'markitdown\[docx,xlsx,pptx\]==0\.1\.0') { throw 'MarkItDown direct dependency is not pinned' }
}
if ($manifest.packId -eq 'office-oxide' -and [string]::IsNullOrWhiteSpace($QualificationFile)) {
  Write-Output 'office-oxide: DISABLED (independent qualification missing)'
  exit 0
}
if ($manifest.packId -eq 'office-oxide') {
  if (-not (Test-Path -LiteralPath $QualificationFile -PathType Leaf)) { throw 'qualification evidence is missing' }
  $qualification = Get-Content -LiteralPath $QualificationFile -Raw | ConvertFrom-Json
  $requiredTriples = @($manifest.targetTriples)
  $qualifiedTriples = @($qualification.qualifiedTargetTriples)
  if ($qualification.schemaVersion -ne 1 -or -not $qualification.criticalAssertionsPassed) { throw 'critical qualification assertions did not pass' }
  if ([uint32]$qualification.securityBlockers -ne 0 -or [uint32]$qualification.fuzzBlockers -ne 0) { throw 'qualification has security or fuzz blockers' }
  foreach ($triple in $requiredTriples) {
    if ($qualifiedTriples -notcontains $triple) { throw "qualification does not cover target: $triple" }
  }
}
if ($ReleaseArchive) {
  if (-not (Test-Path -LiteralPath $ReleaseArchive -PathType Leaf)) { throw 'release archive is missing' }
  $digest = (Get-FileHash -LiteralPath $ReleaseArchive -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($digest -ne $manifest.archiveSha256) { throw 'release archive hash mismatch' }
  if ([uint64]$manifest.compressedBytes -ne (Get-Item -LiteralPath $ReleaseArchive).Length) { throw 'compressed size mismatch' }
  if ([uint64]$manifest.installedBytes -eq 0) { throw 'installed size is not measured' }
  if ([string]::IsNullOrWhiteSpace($manifest.signature)) { throw 'release manifest is unsigned' }
}
Write-Output "$($manifest.packId): declaration verified; release evidence required before publication"

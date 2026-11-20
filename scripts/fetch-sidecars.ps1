# Downloads the tools bundled with the Windows installer and checks their SHA-256:
#
#   src-tauri/binaries/<tool>-x86_64-pc-windows-msvc.exe   sidecars (ffmpeg, ffprobe, qpdf, tesseract)
#   src-tauri/resources/windows/ffmpeg/                    FFmpeg license and build information
#   src-tauri/resources/windows/qpdf/                      qpdf DLLs and license
#   src-tauri/resources/windows/tesseract/                 Tesseract DLLs, license and tessdata
#   src-tauri/resources/pdfium/                            pdfium.dll, VERSION and license
#
# Versions are pinned; update the URL and the hash together. Needs 7-Zip (preinstalled on
# GitHub's Windows runners) to unpack the Tesseract installer without running it.

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
Set-StrictMode -Version Latest

$Triple = 'x86_64-pc-windows-msvc'

$Downloads = @{
  ffmpeg    = @{
    Url    = 'https://github.com/GyanD/codexffmpeg/releases/download/9.0.2/ffmpeg-9.0.2-essentials_build.zip'
    Sha256 = '60f467265b1e312373dbcd92200c2618a74850f98d3d078e94296bb3fa2047ba'
  }
  qpdf      = @{
    Url    = 'https://github.com/qpdf/qpdf/releases/download/v12.4.2/qpdf-12.4.2-msvc64.zip'
    Sha256 = 'db87077e683630c1217e0e8f9a20a9749d952ab676e881c3689187763a5de25d'
  }
  qpdfLicense = @{
    Url    = 'https://raw.githubusercontent.com/qpdf/qpdf/v12.4.2/LICENSE.txt'
    Sha256 = 'cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30'
  }
  qpdfNotice = @{
    Url    = 'https://raw.githubusercontent.com/qpdf/qpdf/v12.4.2/NOTICE.md'
    Sha256 = 'b207f65a9e5491195ded63b2941199b19a4d30148871f2742c88eae7bfc513a6'
  }
  tesseract = @{
    Url    = 'https://github.com/UB-Mannheim/tesseract/releases/download/v5.4.0.20240606/tesseract-ocr-w64-setup-5.4.0.20240606.exe'
    Sha256 = 'c885fff6998e0608ba4bb8ab51436e1c6775c2bafc2559a19b423e18678b60c9'
  }
  tessEng   = @{
    Url    = 'https://github.com/tesseract-ocr/tessdata_fast/raw/4.1.0/eng.traineddata'
    Sha256 = '7d4322bd2a7749724879683fc3912cb542f19906c83bcc1a52132556427170b2'
  }
  tessPor   = @{
    Url    = 'https://github.com/tesseract-ocr/tessdata_fast/raw/4.1.0/por.traineddata'
    Sha256 = 'c4932b937207a9514b7514d518b931a99938c02a28a5a5a553f8599ed58b7deb'
  }
  pdfium    = @{
    Url    = 'https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F8165/pdfium-win-x64.tgz'
    Sha256 = 'ca7338ae7779b8b1b82f4580305c4d762d214f8df1d54b47647fe3cc75989ded'
  }
}

# DLLs tesseract.exe loads (its import closure); the installer also ships training tools
# and their libraries, which are not needed.
$TesseractDlls = @(
  'libLerc.dll', 'libarchive-13.dll', 'libb2-1.dll', 'libbz2-1.dll', 'libcrypto-3-x64.dll',
  'libdeflate.dll', 'libexpat-1.dll', 'libgcc_s_seh-1.dll', 'libgif-7.dll', 'libiconv-2.dll',
  'libjbig-0.dll', 'libjpeg-8.dll', 'libleptonica-6.dll', 'liblz4.dll', 'liblzma-5.dll',
  'libopenjp2-7.dll', 'libpng16-16.dll', 'libsharpyuv-0.dll', 'libstdc++-6.dll',
  'libtesseract-5.dll', 'libtiff-6.dll', 'libwebp-7.dll', 'libwebpmux-3.dll',
  'libwinpthread-1.dll', 'libzstd.dll', 'zlib1.dll'
)

$Root = Split-Path -Parent $PSScriptRoot
$Tauri = Join-Path $Root 'src-tauri'
$Binaries = Join-Path $Tauri 'binaries'
$Windows = Join-Path $Tauri 'resources\windows'
$Pdfium = Join-Path $Tauri 'resources\pdfium'
$Work = Join-Path ([System.IO.Path]::GetTempPath()) ("easy-converter-sidecars-" + [guid]::NewGuid())

function Get-Verified([string] $Name) {
  $item = $Downloads[$Name]
  $file = Join-Path $Work ($Name + '-' + [System.IO.Path]::GetFileName(($item.Url -split '\?')[0]))
  Write-Host "Downloading $Name..."
  Invoke-WebRequest -Uri $item.Url -OutFile $file -UseBasicParsing
  $actual = (Get-FileHash -Algorithm SHA256 -Path $file).Hash.ToLowerInvariant()
  if ($actual -ne $item.Sha256) {
    throw "SHA-256 mismatch for $Name`n  expected $($item.Sha256)`n  got      $actual"
  }
  return $file
}

function Copy-Required([string] $From, [string] $To) {
  if (-not (Test-Path -LiteralPath $From -PathType Leaf)) {
    throw "Expected file not found: $From"
  }
  Copy-Item -LiteralPath $From -Destination $To -Force
}

function Reset-Folder([string] $Path) {
  if (Test-Path -LiteralPath $Path) { Remove-Item -LiteralPath $Path -Recurse -Force }
  New-Item -ItemType Directory -Path $Path -Force | Out-Null
}

$SevenZip = Join-Path $env:ProgramFiles '7-Zip\7z.exe'
$found = Get-Command 7z -ErrorAction SilentlyContinue
if ($found) { $SevenZip = $found.Source }
if (-not (Test-Path -LiteralPath $SevenZip)) { throw '7-Zip is required to unpack the Tesseract installer.' }

New-Item -ItemType Directory -Path $Work -Force | Out-Null
try {
  New-Item -ItemType Directory -Path $Binaries -Force | Out-Null

  # FFmpeg and ffprobe: static GPL v3 build without non-free components.
  $zip = Get-Verified 'ffmpeg'
  $out = Join-Path $Work 'ffmpeg'
  Expand-Archive -LiteralPath $zip -DestinationPath $out
  $build = Get-ChildItem -LiteralPath $out -Directory | Select-Object -First 1
  Copy-Required (Join-Path $build.FullName 'bin\ffmpeg.exe') (Join-Path $Binaries "ffmpeg-$Triple.exe")
  Copy-Required (Join-Path $build.FullName 'bin\ffprobe.exe') (Join-Path $Binaries "ffprobe-$Triple.exe")
  $ffmpegDir = Join-Path $Windows 'ffmpeg'
  Reset-Folder $ffmpegDir
  Copy-Required (Join-Path $build.FullName 'LICENSE') (Join-Path $ffmpegDir 'LICENSE.txt')
  Copy-Required (Join-Path $build.FullName 'README.txt') (Join-Path $ffmpegDir 'BUILD-INFO.txt')

  # qpdf: the executable is a sidecar; its DLLs go to the resource folder.
  $zip = Get-Verified 'qpdf'
  $out = Join-Path $Work 'qpdf'
  Expand-Archive -LiteralPath $zip -DestinationPath $out
  $bin = Join-Path (Get-ChildItem -LiteralPath $out -Directory | Select-Object -First 1).FullName 'bin'
  Copy-Required (Join-Path $bin 'qpdf.exe') (Join-Path $Binaries "qpdf-$Triple.exe")
  $qpdfDir = Join-Path $Windows 'qpdf'
  Reset-Folder $qpdfDir
  Get-ChildItem -LiteralPath $bin -Filter '*.dll' | Copy-Item -Destination $qpdfDir
  Copy-Required (Get-Verified 'qpdfLicense') (Join-Path $qpdfDir 'LICENSE.txt')
  Copy-Required (Get-Verified 'qpdfNotice') (Join-Path $qpdfDir 'NOTICE.md')

  # Tesseract: unpack the installer (never run it) and keep what tesseract.exe needs.
  $setup = Get-Verified 'tesseract'
  $out = Join-Path $Work 'tesseract'
  & $SevenZip x -y "-o$out" $setup | Out-Null
  if ($LASTEXITCODE -ne 0) { throw "7-Zip failed with exit code $LASTEXITCODE" }
  Copy-Required (Join-Path $out 'tesseract.exe') (Join-Path $Binaries "tesseract-$Triple.exe")
  $tessDir = Join-Path $Windows 'tesseract'
  Reset-Folder $tessDir
  foreach ($dll in $TesseractDlls) { Copy-Required (Join-Path $out $dll) $tessDir }
  Copy-Required (Join-Path $out 'doc\LICENSE') (Join-Path $tessDir 'LICENSE.txt')
  $tessdata = Join-Path $tessDir 'tessdata'
  New-Item -ItemType Directory -Path (Join-Path $tessdata 'configs') -Force | Out-Null
  # Needed for PDF output (OCR of PDF pages).
  Copy-Required (Join-Path $out 'tessdata\pdf.ttf') $tessdata
  Copy-Required (Join-Path $out 'tessdata\configs\pdf') (Join-Path $tessdata 'configs')
  Copy-Required (Get-Verified 'tessEng') (Join-Path $tessdata 'eng.traineddata')
  Copy-Required (Get-Verified 'tessPor') (Join-Path $tessdata 'por.traineddata')

  # PDFium.
  $tgz = Get-Verified 'pdfium'
  $out = Join-Path $Work 'pdfium'
  New-Item -ItemType Directory -Path $out -Force | Out-Null
  tar -xzf $tgz -C $out
  if ($LASTEXITCODE -ne 0) { throw "tar failed with exit code $LASTEXITCODE" }
  Reset-Folder $Pdfium
  Copy-Required (Join-Path $out 'bin\pdfium.dll') $Pdfium
  Copy-Required (Join-Path $out 'VERSION') $Pdfium
  Copy-Required (Join-Path $out 'LICENSE') $Pdfium

  Write-Host 'Sidecars and resources are ready.'
}
finally {
  Remove-Item -LiteralPath $Work -Recurse -Force -ErrorAction SilentlyContinue
}

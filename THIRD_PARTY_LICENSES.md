# Licenças de terceiros

O Easy Converter é distribuído sob a [GPL-3.0](LICENSE). Este arquivo lista os componentes de terceiros que acompanham os pacotes, com licença, versão exata e onde obter o código-fonte.

As ferramentas externas rodam como **processos separados** (sidecars ou programas do sistema); nenhuma biblioteca GPL é ligada ao código do app.

## Binários distribuídos

### Em todos os pacotes (`.deb`, `.rpm`, `.msi`, `.exe`)

| Componente | Versão | Licença | Código-fonte |
|---|---|---|---|
| PDFium (build de [pdfium-binaries](https://github.com/bblanchon/pdfium-binaries)) | chromium/8165 | BSD-3-Clause | https://pdfium.googlesource.com/pdfium/+/refs/heads/chromium/8165 · build: https://github.com/bblanchon/pdfium-binaries/releases/tag/chromium%2F8165 |

O PDFium inclui código de terceiros (FreeType, libjpeg-turbo, Little CMS, OpenJPEG, zlib, Abseil e outros), sob licenças permissivas. A lista completa está em `third_party/` no código-fonte do PDFium. O texto da licença vai junto com o app, na pasta `pdfium/` dos recursos.

### Somente no instalador do Windows

No Linux, FFmpeg, qpdf e Tesseract **não** vêm no pacote: são dependências instaladas pelo `apt` ou `dnf`, a partir dos repositórios da distribuição.

| Componente | Versão | Licença | Código-fonte |
|---|---|---|---|
| FFmpeg e ffprobe (build estática "essentials" de [gyan.dev](https://www.gyan.dev/ffmpeg/builds/)) | 9.0.2 | GPL-3.0-or-later (build com `--enable-gpl --enable-version3`, **sem** `--enable-nonfree`) | https://github.com/FFmpeg/FFmpeg/tree/n9.0.2 (commit `946fcce07b`) · build: https://github.com/GyanD/codexffmpeg/releases/tag/9.0.2 |
| qpdf | 12.4.2 | Apache-2.0 | https://github.com/qpdf/qpdf/tree/v12.4.2 |
| Microsoft Visual C++ Runtime (DLLs que acompanham o qpdf) | 14.x | [Licença de redistribuição da Microsoft](https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files) | Não se aplica |
| Tesseract OCR (build da [UB Mannheim](https://github.com/UB-Mannheim/tesseract)) | 5.4.0.20240606 | Apache-2.0 | https://github.com/tesseract-ocr/tesseract/tree/5.4.0 · build: https://github.com/UB-Mannheim/tesseract/releases/tag/v5.4.0.20240606 |
| Dados de idioma do Tesseract (`por`, `eng`), tessdata_fast | 4.1.0 | Apache-2.0 | https://github.com/tesseract-ocr/tessdata_fast/tree/4.1.0 |

Os arquivos são baixados por `scripts/fetch-sidecars.ps1`, que confere o SHA-256 de cada um.

**FFmpeg (GPL-3.0).** A build inclui bibliotecas externas (por exemplo x264, x265, libvpx, libaom, LAME, Opus). A lista completa e a configuração exata vão junto com o app, no arquivo `ffmpeg/BUILD-INFO.txt` dos recursos, junto com o texto da licença (`ffmpeg/LICENSE.txt`). O código-fonte correspondente está no commit do FFmpeg indicado acima e nos projetos de cada biblioteca, nas versões listadas no `BUILD-INFO.txt`. Se algum desses links deixar de funcionar, abra uma issue no repositório do projeto, e o código-fonte correspondente será disponibilizado, conforme a seção 6 da GPL-3.0.

**Tesseract.** O instalador do app leva só o `tesseract.exe` e as bibliotecas que ele carrega, todas da build da UB Mannheim (pacotes MSYS2):

| Biblioteca | Licença |
|---|---|
| Leptonica | BSD-2-Clause |
| libarchive | BSD-2-Clause |
| libtiff | libtiff (permissiva) |
| libpng | libpng-2.0 |
| libjpeg-turbo | IJG / BSD-3-Clause / Zlib |
| libwebp, libsharpyuv | BSD-3-Clause |
| OpenJPEG | BSD-2-Clause |
| giflib | MIT |
| JBIG-KIT | GPL-2.0-or-later |
| LERC | Apache-2.0 |
| libdeflate | MIT |
| zlib | Zlib |
| bzip2 | bzip2-1.0.6 |
| XZ Utils (liblzma) | 0BSD |
| LZ4 | BSD-2-Clause |
| Zstandard | BSD-3-Clause |
| BLAKE2 (libb2) | CC0-1.0 |
| Expat | MIT |
| GNU libiconv | LGPL-2.1-or-later |
| OpenSSL (libcrypto) | Apache-2.0 |
| GCC runtime (libgcc, libstdc++) | GPL-3.0 com GCC Runtime Library Exception |
| MinGW-w64 winpthreads | MIT / BSD |

O código-fonte de cada uma está nos pacotes MSYS2 usados pela build (https://github.com/msys2/MINGW-packages) e no repositório da UB Mannheim.

## Ferramentas opcionais (não distribuídas)

Usadas somente se o usuário já as tiver instalado: LibreOffice (MPL-2.0), Pandoc (GPL-2.0-or-later), Calibre (GPL-3.0), Ghostscript (AGPL-3.0), 7-Zip (LGPL-2.1 com restrição unRAR).

## Bibliotecas compiladas no app

O app é construído com estas bibliotecas (dependências diretas), todas com licenças compatíveis com a GPL-3.0:

| Biblioteca | Versão | Licença |
|---|---|---|
| Tauri e plugins (dialog, opener, store) | 2.12.7 / 2.8.4 / 2.7.3 / 2.5.1 | MIT ou Apache-2.0 |
| image | 0.25.10 | MIT ou Apache-2.0 |
| Tokio | 1.54.1 | MIT |
| Serde, serde_json | 1.0.229 / 1.0.154 | MIT ou Apache-2.0 |
| thiserror | 2.0.21 | MIT ou Apache-2.0 |
| ts-rs | 12.0.1 | MIT |
| Vue | 3.5.45 | MIT |
| Vue Router | 5.5.1 | MIT |
| Pinia | 4.1.0 | MIT |
| Vue I18n | 11.4.23 | MIT |
| @tauri-apps/api, @tauri-apps/plugin-dialog | 2.12.5 / 2.8.4 | MIT ou Apache-2.0 |

As dependências indiretas usam MIT, Apache-2.0, BSD, ISC, Zlib, Unicode-3.0, CC0, Unlicense, 0BSD ou MPL-2.0. A lista completa sai de `cargo tree` (em `src-tauri/`) e do `package-lock.json`.

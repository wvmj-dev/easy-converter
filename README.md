# Easy Converter

> Nome provisório.

App desktop para **converter e editar arquivos localmente**: imagens, PDF, vídeo, áudio, documentos, planilhas, apresentações, legendas, ebooks e arquivos compactados.

Uma alternativa a sites como Convertio, iLovePDF e iLoveIMG, com uma diferença: **nenhum arquivo sai do seu computador**. O app não faz nenhuma requisição de rede, não tem telemetria, analytics nem atualização automática.

- Gratuito e de código aberto, sob a licença [GPL-3.0](LICENSE).
- Projeto pessoal, sem fins lucrativos, mantido nas horas livres do autor.
- Nunca sobrescreve o arquivo original.

> **Situação:** a versão 0.1.0 converte e edita imagens (PNG, JPG, WebP, BMP, GIF, TIFF e ICO). A próxima, 0.2.0, traz PDF: juntar, dividir, extrair e reordenar páginas, girar, comprimir, converter para imagens e a partir delas, extrair texto e imagens, OCR e senhas. Vídeo, áudio, documentos e os demais formatos chegam depois.

## Aviso sobre uso de IA

Este projeto foi desenvolvido com auxílio de ferramentas de inteligência artificial.
O código pode conter erros, falhas de segurança ou comportamentos inesperados.
Revise o código antes de usar com arquivos importantes e mantenha backups dos seus dados.
O software é fornecido sem garantias, conforme a licença GPL-3.0.

## Plataformas

| Plataforma | Pacote |
|---|---|
| Debian 12+, Ubuntu 22.04+ | `.deb` |
| Fedora (versões suportadas) | `.rpm` |
| Windows 10/11 64 bits | `.msi` ou `.exe` |

Android, iOS e macOS não são suportados.

## Instalação

Baixe o pacote do seu sistema na página de releases do GitHub.

### Debian / Ubuntu

```bash
sudo apt install ./Easy.Converter_<versão>_amd64.deb
```

### Fedora

```bash
sudo dnf install ./Easy.Converter-<versão>-1.x86_64.rpm
```

Use `apt` ou `dnf` como acima: eles instalam automaticamente as dependências (FFmpeg, qpdf e Tesseract). O `dpkg -i` não resolve dependências e não é recomendado.

No Fedora, o pacote `ffmpeg-free` não inclui todos os codificadores (por exemplo, H.264 via x264). Para ter o FFmpeg completo, use o RPM Fusion:

```bash
sudo dnf swap ffmpeg-free ffmpeg --allowerasing
```

### Windows

Baixe o instalador `Easy.Converter_<versão>_x64-setup.exe` (ou o `.msi`). As ferramentas obrigatórias já vêm dentro do instalador.

O app não tem assinatura de código, então o Windows SmartScreen exibe um aviso na primeira execução. Para continuar, clique em **Mais informações** e depois em **Executar assim mesmo**.

## Ferramentas opcionais

Algumas funções dependem de programas que não vêm com o app. O app detecta se estão instalados e desativa a função correspondente quando faltam. A tela **Ferramentas** mostra o que foi encontrado.

| Ferramenta | Para quê |
|---|---|
| LibreOffice | Documentos, planilhas e apresentações (DOCX, XLSX, PPTX, ODT etc.) |
| Pandoc | Markdown, HTML e TXT ↔ DOCX |
| Calibre | Ebooks (EPUB, MOBI, AZW3) |
| Ghostscript | Compressão forte de PDF |
| 7-Zip | Leitura de arquivos RAR |

Debian / Ubuntu:

```bash
sudo apt install libreoffice-writer libreoffice-calc libreoffice-impress \
  pandoc calibre ghostscript 7zip
```

Fedora:

```bash
sudo dnf install libreoffice-writer libreoffice-calc libreoffice-impress \
  pandoc calibre ghostscript 7zip
```

Suporte a RAR: no Debian, fica no pacote `7zip-rar` (repositório non-free); o pacote `7zip` oficial do Fedora não lê RAR.

## Desenvolvimento

### Programas necessários

| Grupo | Para quê | Obrigatório? |
|---|---|---|
| Dependências do Tauri 2 | Compilar o app (WebKitGTK, GTK, OpenSSL etc.) | Sim |
| Rust (com `rustfmt` e `clippy`) e Node.js LTS | Compilar o backend e o frontend | Sim |
| FFmpeg, qpdf, Tesseract (com idiomas `por` e `eng`) | Ferramentas obrigatórias do app (vídeo, áudio, PDF e OCR); também usadas nos testes | Sim |
| ExifTool | Conferir metadados nos testes | Sim, para os testes |
| LibreOffice, Pandoc, Calibre, Ghostscript, 7-Zip | Ferramentas opcionais (veja acima); os testes que dependem delas são ignorados quando faltam | Não |
| Xvfb, WebKitWebDriver, xdotool, `tauri-driver` | Abrir e controlar o app real em testes automatizados (tela virtual, cliques) | Não |
| PDFium | Renderização de PDF; baixado por `scripts/fetch-pdfium.sh`, não vem do `apt`/`dnf` | Sim (necessário para compilar) |

#### Debian / Ubuntu

```bash
# Dependências do Tauri 2
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev

# Ferramentas obrigatórias do app e ExifTool (testes)
sudo apt install ffmpeg qpdf tesseract-ocr tesseract-ocr-por tesseract-ocr-eng \
  libimage-exiftool-perl

# Ferramentas opcionais (7zip-rar fica no repositório non-free)
sudo apt install libreoffice-writer libreoffice-calc libreoffice-impress \
  pandoc calibre ghostscript 7zip 7zip-rar

# Opcional: testes automatizados da interface
sudo apt install xvfb webkit2gtk-driver xdotool
cargo install tauri-driver --locked
```

#### Fedora

```bash
# Dependências do Tauri 2
sudo dnf check-update
sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file \
  libappindicator-gtk3-devel librsvg2-devel libxdo-devel
sudo dnf group install "c-development"

# Ferramentas obrigatórias do app e ExifTool (testes)
sudo dnf install ffmpeg-free qpdf tesseract tesseract-langpack-por tesseract-langpack-eng \
  perl-Image-ExifTool

# Ferramentas opcionais (o 7zip do Fedora não lê RAR)
sudo dnf install libreoffice-writer libreoffice-calc libreoffice-impress \
  pandoc calibre ghostscript 7zip

# Opcional: testes automatizados da interface
# (confirme o pacote do WebKitWebDriver com: dnf provides '*/WebKitWebDriver')
sudo dnf install xorg-x11-server-Xvfb xdotool
cargo install tauri-driver --locked
```

#### Rust e Node.js (todas as distros)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup component add rustfmt clippy

curl -fsSL https://fnm.vercel.app/install | bash
fnm install --lts
```

Confira os nomes dos pacotes antes de cada release com `apt-cache policy <pacote>` ou `dnf info <pacote>`. A tela **Ferramentas** do app mostra o que foi encontrado.

### Comandos

```bash
npm install
./scripts/fetch-pdfium.sh   # uma vez: baixa o PDFium, necessário para compilar
npm run tauri dev      # executa o app em modo de desenvolvimento
npm run tauri build    # gera os pacotes
```

Verificações:

```bash
npm run lint
npm run typecheck
npm run test
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

O build do Rust precisa da pasta `dist/`; rode `npm run build` antes do `cargo clippy` ou `cargo test` se ela ainda não existir.

Os tipos TypeScript em `src/types/` são gerados a partir do Rust (com `ts-rs`) quando o `cargo test` roda. Depois de mudar um tipo exportado no Rust, rode `cargo test` e faça commit dos arquivos gerados; o CI falha se eles estiverem desatualizados.

### Publicar uma versão

1. Atualize a versão em `package.json`, `package-lock.json` e `src-tauri/Cargo.toml` (o `tauri.conf.json` lê a do `package.json`).
2. Faça o commit e crie a tag: `git tag v<versão>` e `git push origin v<versão>`.
3. O workflow `release.yml` gera os pacotes `.deb`, `.rpm`, `.msi` e `.exe` e os anexa a um **rascunho** de release no GitHub.
4. Revise o rascunho e publique.

As versões dos binários incluídos ficam fixas nos scripts de `scripts/`, com SHA-256. Ao atualizar uma delas, atualize também o [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).

## Contribuindo

Veja [CONTRIBUTING.md](CONTRIBUTING.md).

## Licença

[GPL-3.0](LICENSE). As ferramentas externas usadas pelo app têm licenças próprias; veja [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).

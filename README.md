# Easy Converter

> Nome provisório.

App desktop para **converter e editar arquivos localmente**: imagens, PDF, vídeo, áudio, documentos, planilhas, apresentações, legendas, ebooks e arquivos compactados.

Uma alternativa a sites como Convertio, iLovePDF e iLoveIMG, com uma diferença: **nenhum arquivo sai do seu computador**. O app não faz nenhuma requisição de rede, não tem telemetria, analytics nem atualização automática.

- Gratuito e de código aberto, sob a licença [GPL-3.0](LICENSE).
- Projeto pessoal, sem fins lucrativos, mantido nas horas livres do autor.
- Nunca sobrescreve o arquivo original.

> **Situação:** em desenvolvimento inicial. Ainda não há versões publicadas.

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

Os pacotes serão publicados na página de releases do GitHub.

### Debian / Ubuntu

```bash
sudo apt install ./app_<versao>_amd64.deb
```

### Fedora

```bash
sudo dnf install ./app-<versao>-1.x86_64.rpm
```

Use `apt` ou `dnf` como acima: eles instalam automaticamente as dependências (FFmpeg, qpdf e Tesseract). O `dpkg -i` não resolve dependências e não é recomendado.

No Fedora, o pacote `ffmpeg-free` não inclui todos os codificadores (por exemplo, H.264 via x264). Para ter o FFmpeg completo, use o RPM Fusion:

```bash
sudo dnf swap ffmpeg-free ffmpeg --allowerasing
```

### Windows

Baixe o instalador `.msi` ou `.exe`. As ferramentas obrigatórias já vêm dentro do instalador.

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

Requisitos: Rust (com `rustfmt` e `clippy`), Node.js LTS e as dependências de sistema do Tauri 2. Veja o [guia do Tauri](https://v2.tauri.app/start/prerequisites/).

```bash
npm install
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

## Contribuindo

Veja [CONTRIBUTING.md](CONTRIBUTING.md).

## Licença

[GPL-3.0](LICENSE). As ferramentas externas usadas pelo app têm licenças próprias.

# CLAUDE.md

Contexto e regras do projeto para o Claude Code. Leia este arquivo inteiro antes de qualquer tarefa.

---

## 1. Visão geral

App desktop para **converter e editar arquivos localmente**, sem enviar nada para a nuvem. Substitui o uso de sites como Convertio, iLovePDF e iLoveIMG.

- **Gratuito e aberto**, licença **GPL-3.0**.
- **Projeto pessoal**, mantido nas horas livres do autor. Sem doações, sem links de financiamento, sem `FUNDING.yml`.
- Desenvolvido **com auxílio de IA**. O README deve deixar isso claro, com aviso de que o código pode conter erros e é fornecido sem garantia.
- Contribuições são bem-vindas, mas **sem compromisso** de revisão, resposta ou aceitação.

### Plataformas

| Plataforma | Formato | Situação |
|---|---|---|
| Linux base Debian (Debian 12+, Ubuntu 22.04+) | `.deb` | No escopo |
| Linux base Fedora (versões suportadas) | `.rpm` | No escopo |
| Windows 10/11 64 bits | `.msi` e `.exe` (NSIS) | No escopo |
| Android, iOS, macOS | | **Fora do escopo** |

### Stack

- **Tauri 2** (backend em Rust)
- **Vue 3 + TypeScript + Vite** (frontend)
- **Pinia**, **vue-router**, **vue-i18n**
- Persistência: **`tauri-plugin-store`** (JSON). **Sem SQLite** no MVP. Só considerar SQLite se surgir histórico ou presets em grande volume, e consultar o autor antes.

---

## 2. Princípios inegociáveis

1. **Nenhum arquivo sai do computador.** Nada de telemetria, analytics, crash reporting, updater, CDN ou qualquer requisição de rede.
2. **Nunca sobrescrever o original.** Saída em pasta escolhida ou com sufixo (`arquivo-convertido.ext`). Se existir, numerar (`arquivo-convertido (2).ext`).
3. **Processar em pasta temporária** e mover para o destino só ao final. Limpar temporários ao concluir, cancelar ou fechar o app.
4. **O frontend nunca lê o conteúdo dos arquivos.** Ele só manipula caminhos e chama comandos do backend.
5. **Processos externos só são iniciados pelo Rust**, com argumentos em lista. Nunca montar string de shell.
6. **Validar no Rust** todos os caminhos, extensões e parâmetros (qualidade, resolução, intervalos de páginas e de tempo).

---

## 3. Regras para o Claude Code

- **Não implementar nada além do que foi pedido.** Se notar algo que deveria mudar fora do escopo da tarefa, descrever e perguntar antes.
- **Não adicionar dependências** (crates, pacotes npm, plugins Tauri) sem justificar e perguntar, a menos que a dependência já esteja listada neste arquivo.
- **Nunca adicionar** `tauri-plugin-http`, `tauri-plugin-updater` ou qualquer biblioteca que faça requisições de rede.
- **Nunca commitar binários** (sidecars, PDFium, DLLs, traineddata). Eles são baixados por script.
- Ao criar uma nova operação, seguir o padrão existente: variante em `JobRequest`, função em `ops/<categoria>.rs`, tipos gerados para TypeScript, formulário em `components/options/`, teste de integração com fixture.
- Antes de concluir uma tarefa, rodar: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `npm run lint`, `npm run typecheck`, `npm run test`.

### Commits

- **Semânticos** (Conventional Commits): `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `build`, `ci`, `perf`, `style`.
- Título no formato `tipo(escopo opcional): resumo`.
- **Um commit por assunto**, a menos que o autor peça o contrário.
- Corpo descrevendo **o que** foi alterado e **por quê**.
- **Em inglês**, por padrão.
- **Não mencionar IA** e **não incluir IA como coautor** (sem `Co-Authored-By` de IA, sem trailers de sessão), a menos que o autor peça.

### Convenções de código

- Identificadores, comentários e mensagens de log em **inglês**.
- Textos da interface **sempre via i18n** (`src/i18n/*.json`), nunca escritos direto no template.
- Rust: `rustfmt` padrão, `clippy` sem warnings, erros via `AppError` (sem `unwrap()` fora de testes).
- TypeScript: `strict: true`, sem `any` implícito, Composition API com `<script setup lang="ts">`.

---

## 4. Licenças

- O app é **GPL-3.0**. Todas as dependências previstas são compatíveis.
- Ferramentas externas rodam como **processos separados** (sidecar ou binário do sistema). Não linkar bibliotecas GPL diretamente no código Rust.
- FFmpeg: usar builds **sem `--enable-nonfree`**.
- Manter `THIRD_PARTY_LICENSES.md` com licença e link do código-fonte de cada binário distribuído, na versão exata.
- A tela "Sobre" mostra a licença do app, o aviso de IA e os créditos das dependências.

| Ferramenta | Licença |
|---|---|
| FFmpeg | LGPL 2.1+ ou GPL, conforme a build |
| qpdf | Apache 2.0 |
| PDFium | BSD-3-Clause |
| Tesseract | Apache 2.0 |
| LibreOffice | MPL 2.0 |
| Pandoc | GPL 2+ |
| Calibre | GPL 3 |
| Ghostscript | AGPL 3 |
| 7-Zip | LGPL + restrição unRAR |
| Tauri | MIT ou Apache 2.0 |

---

## 5. Arquitetura

### 5.1 Três formas de executar uma operação

| Forma | Quando | Exemplos |
|---|---|---|
| **Crate Rust** | Operações simples, sem dependência externa | `image`, `lopdf`, `zip`, `tar`, `flate2`, `sevenz-rust`, `csv`, `serde_json` |
| **Ferramenta obrigatória** | Sempre disponível (dependência do pacote no Linux, sidecar no Windows) | FFmpeg, ffprobe, qpdf, Tesseract, PDFium |
| **Ferramenta opcional** | Detectada no sistema; função desabilitada se faltar | LibreOffice, Pandoc, Calibre, Ghostscript, 7-Zip |

### 5.2 Resolvedor de ferramentas (`tools/resolver.rs`)

Na inicialização, para cada ferramenta:

1. Procura o sidecar empacotado.
2. Se não achar, procura no `PATH`.
3. Se não achar, usa o caminho configurado pelo usuário nas configurações.
4. Executa a versão (`--version`, `-version`) e guarda versão e recursos:
   - FFmpeg: lista de encoders (`ffmpeg -encoders`), porque o `ffmpeg-free` do Fedora não tem todos (ex.: x264).
   - Tesseract: idiomas instalados (`tesseract --list-langs`).

O comando `get_tools_status` expõe isso ao frontend, que habilita ou desabilita funções e mostra a tela "Ferramentas" com instruções de instalação.

### 5.3 Fila de tarefas (`jobs/`)

- `Job`: `id`, `kind`, `inputs`, `output`, `status` (`queued`, `running`, `done`, `failed`, `cancelled`), `progress` (0 a 100 ou indeterminado).
- Concorrência configurável. Padrão: núcleos / 2. Vídeo: 1 por vez por padrão.
- **Cancelamento:** guardar o handle do processo filho e encerrá-lo. Em operações com crate, token de cancelamento verificado entre etapas.
- **Progresso:**
  - FFmpeg: `-progress pipe:1`, comparando `out_time_ms` com a duração obtida via `ffprobe`.
  - PDF e lotes: páginas ou arquivos concluídos.
  - LibreOffice e Pandoc: indeterminado.
- **Timeout** configurável por tipo de operação.
- Eventos: `job:created`, `job:progress`, `job:finished`, `job:failed`.

### 5.4 Comandos Tauri

```rust
#[tauri::command]
async fn enqueue_job(request: JobRequest, state: State<'_, JobQueue>) -> Result<JobId, AppError>;

#[tauri::command]
async fn cancel_job(id: JobId, state: State<'_, JobQueue>) -> Result<(), AppError>;

#[tauri::command]
async fn get_tools_status(state: State<'_, ToolRegistry>) -> Result<Vec<ToolStatus>, AppError>;

#[tauri::command]
async fn probe_file(path: PathBuf) -> Result<FileInfo, AppError>;
```

`JobRequest` é um enum com uma variante por operação (`ImageResize { .. }`, `PdfMerge { .. }`, `VideoConvert { .. }` etc.), com `serde`. Os tipos TypeScript são gerados com `ts-rs` ou `specta` para `src/types/`.

### 5.5 Estrutura do backend

```
src-tauri/
├── Cargo.toml
├── tauri.conf.json
├── tauri.linux.conf.json
├── tauri.windows.conf.json
├── capabilities/
│   └── default.json
├── binaries/              # sidecars do Windows (fora do git)
├── resources/             # PDFium, DLLs, traineddata (fora do git)
└── src/
    ├── main.rs
    ├── lib.rs
    ├── error.rs
    ├── settings.rs
    ├── fs_utils.rs
    ├── pdf_worker.rs      # PDFium isolado em processo separado
    ├── tools/
    │   ├── mod.rs
    │   ├── resolver.rs
    │   ├── capabilities.rs
    │   ├── ffmpeg.rs
    │   ├── qpdf.rs
    │   ├── tesseract.rs
    │   ├── libreoffice.rs
    │   ├── pandoc.rs
    │   ├── calibre.rs
    │   ├── ghostscript.rs
    │   └── sevenzip.rs
    ├── jobs/
    │   ├── mod.rs
    │   ├── queue.rs
    │   ├── job.rs
    │   └── events.rs
    └── ops/
        ├── image.rs
        ├── pdf.rs
        ├── video.rs
        ├── audio.rs
        ├── document.rs
        ├── spreadsheet.rs
        ├── presentation.rs
        ├── subtitle.rs
        ├── ebook.rs
        └── archive.rs
```

### 5.6 Estrutura do frontend

```
src/
├── main.ts
├── App.vue
├── router/index.ts
├── views/
│   ├── HomeView.vue
│   ├── ImageView.vue
│   ├── PdfView.vue
│   ├── VideoView.vue
│   ├── AudioView.vue
│   ├── DocumentView.vue
│   ├── SpreadsheetView.vue
│   ├── PresentationView.vue
│   ├── SubtitleView.vue
│   ├── EbookView.vue
│   ├── ArchiveView.vue
│   ├── ToolsView.vue
│   ├── SettingsView.vue
│   └── AboutView.vue
├── components/
│   ├── DropZone.vue
│   ├── FileList.vue
│   ├── OperationPicker.vue
│   ├── options/
│   ├── JobPanel.vue
│   ├── JobItem.vue
│   └── ToolMissingNotice.vue
├── composables/
│   ├── useJobs.ts
│   ├── useTools.ts
│   └── useDragDrop.ts
├── stores/
│   ├── jobs.ts
│   ├── tools.ts
│   └── settings.ts
├── services/api.ts        # wrappers tipados de invoke
├── types/                 # gerados pelo ts-rs ou specta
└── i18n/
    ├── pt-BR.json
    └── en.json
```

Dependências do frontend: `vue-router`, `pinia`, `vue-i18n`, `@tauri-apps/api`, `@tauri-apps/plugin-dialog`, `@tauri-apps/plugin-store`, `@tauri-apps/plugin-opener`. Recursos (fontes, ícones) empacotados localmente.

### 5.7 Fluxo de uso

1. Usuário arrasta arquivos (evento de drag and drop do Tauri, que entrega caminhos) ou usa o diálogo.
2. `probe_file` identifica tipo, tamanho, páginas, duração, codecs.
3. O app mostra as operações compatíveis; o usuário ajusta as opções.
4. `enqueue_job` coloca a tarefa na fila; o `JobPanel` mostra progresso e botão de cancelar.
5. Ao concluir: "Abrir arquivo" e "Abrir pasta" (`tauri-plugin-opener`).

---

## 6. Segurança e privacidade

| Medida | Como |
|---|---|
| Sem rede no frontend | CSP `default-src 'self'`; sem `connect-src` externo |
| Sem plugins de rede | Nunca instalar `tauri-plugin-http` nem `tauri-plugin-updater` |
| Capabilities mínimas | Frontend só tem `invoke` dos comandos do app, diálogo, store e opener |
| Shell só no Rust | Frontend sem permissão de shell |
| Argumentos seguros | Sempre lista de argumentos |
| Entrada maliciosa | Ferramentas em processo separado, com timeout |

---

## 7. Funcionalidades por fase

### Fase 1: Imagens (crate `image`)
- Converter PNG, JPG, WebP, BMP, GIF, TIFF, ICO
- Redimensionar, cortar, girar, espelhar
- Comprimir com qualidade ajustável
- Remover metadados EXIF
- Juntar imagens lado a lado ou empilhadas
- Depois: HEIC, AVIF e RAW (exigem libheif ou libraw; consultar antes)

### Fase 2: PDF (PDFium, qpdf, lopdf, Tesseract)
- Juntar, dividir, extrair, reordenar e girar páginas
- PDF → imagens (PDFium)
- Imagens → PDF
- Extrair texto e imagens
- Proteger e remover senha (qpdf)
- Compressão estrutural (qpdf); compressão forte com Ghostscript, se instalado
- OCR: renderizar páginas com PDFium → `tesseract pagina.png saida pdf` → juntar com qpdf. Sem OCRmyPDF.

### Fase 3: Vídeo e áudio (FFmpeg)
- Converter e remuxar (detectar quando `-c copy` é possível)
- Cortar por intervalo
- Comprimir com presets (alta, média, baixa)
- Redimensionar
- Extrair áudio
- Vídeo → GIF
- Juntar vídeos
- Remover ou trocar trilha de áudio
- Áudio: converter, cortar, normalizar (`loudnorm`), mono 16 kHz

### Fase 4: Documentos, planilhas e apresentações (LibreOffice e Pandoc, opcionais)
- DOCX, ODT, DOC, RTF ↔ PDF
- XLSX, ODS, CSV entre si e para PDF
- PPTX, ODP → PDF e → imagens (PDF intermediário + PDFium)
- Markdown, HTML, TXT ↔ DOCX (Pandoc)
- CSV ↔ JSON (Rust)
- LibreOffice: perfil temporário por execução (`-env:UserInstallation=file:///...`) ou processamento em série

### Fase 5: Legendas, ebooks e compactados
- Legendas: SRT ↔ VTT ↔ ASS, deslocar tempos, incorporar no vídeo
- Ebooks: EPUB, MOBI, AZW3, PDF via `ebook-convert` (Calibre, opcional)
- Compactados: ZIP, TAR (Rust), 7z (`sevenz-rust`); RAR só leitura via 7-Zip do sistema, se tiver suporte

---

## 8. Empacotamento

### 8.1 O que vai em cada pacote

| Dependência | `.deb` | `.rpm` | `.msi` / `.exe` |
|---|---|---|---|
| WebKitGTK, GTK 3 | Declarada pelo Tauri | Declarada pelo Tauri | Não se aplica |
| WebView2 | Não se aplica | Não se aplica | Instalador (`downloadBootstrapper`) |
| FFmpeg, qpdf, Tesseract | `depends` | `depends` | Sidecars dentro do instalador |
| PDFium | Recurso dentro do pacote | Recurso dentro do pacote | Recurso dentro do instalador |
| Ferramentas opcionais | Não vêm | Não vêm | Não vêm |

O PDFium **não existe** nos repositórios do Debian nem do Fedora, por isso vai empacotado em todas as plataformas (binários de https://github.com/bblanchon/pdfium-binaries).

### 8.2 Configuração

O Tauri 2 mescla `tauri.linux.conf.json` e `tauri.windows.conf.json` com o `tauri.conf.json`.

`tauri.conf.json` (trecho):

```json
{
  "bundle": {
    "active": true,
    "resources": {
      "resources/pdfium/": "pdfium/"
    }
  }
}
```

`tauri.linux.conf.json`:

```json
{
  "bundle": {
    "targets": ["deb", "rpm"],
    "linux": {
      "deb": {
        "depends": [
          "ffmpeg",
          "qpdf",
          "tesseract-ocr",
          "tesseract-ocr-por",
          "tesseract-ocr-eng"
        ]
      },
      "rpm": {
        "depends": [
          "(ffmpeg-free or ffmpeg)",
          "qpdf",
          "tesseract",
          "tesseract-langpack-por",
          "tesseract-langpack-eng"
        ]
      }
    }
  }
}
```

Verificar no primeiro build com `rpm -qpR <arquivo>.rpm` que a dependência booleana foi gravada corretamente.

`tauri.windows.conf.json`:

```json
{
  "bundle": {
    "targets": ["msi", "nsis"],
    "externalBin": [
      "binaries/ffmpeg",
      "binaries/ffprobe",
      "binaries/qpdf",
      "binaries/tesseract"
    ],
    "resources": {
      "resources/windows/ffmpeg/": "ffmpeg/",
      "resources/windows/qpdf/": "qpdf/",
      "resources/windows/tesseract/": "tesseract/"
    },
    "windows": {
      "webviewInstallMode": { "type": "downloadBootstrapper" }
    }
  }
}
```

### 8.3 Windows

- Sidecars nomeados com target triple (ex.: `ffmpeg-x86_64-pc-windows-msvc.exe`).
- DLLs do qpdf e do Tesseract vão em `resources`; o Rust inicia esses processos com diretório de trabalho e `PATH` apontando para a pasta das DLLs.
- `por.traineddata` e `eng.traineddata` em `resources`; definir `TESSDATA_PREFIX` ao chamar o Tesseract.
- Do instalador do Tesseract (UB Mannheim) vão só o `tesseract.exe`, as DLLs que ele importa, `pdf.ttf` e `configs/pdf`; as ferramentas de treino ficam de fora.
- Textos de licença e informações de build vão junto, em `resources/windows/<ferramenta>/`.
- `scripts/fetch-sidecars.ps1` baixa versões fixas e confere SHA-256.
- Sem assinatura de código: o SmartScreen avisa na primeira execução. Explicar no README.

### 8.4 Instalação no Linux (para o README)

```bash
# Debian / Ubuntu
sudo apt install ./app_<versao>_amd64.deb

# Fedora
sudo dnf install ./app-<versao>-1.x86_64.rpm
```

`dpkg -i` não resolve dependências; não recomendar.

---

## 9. Pré-requisitos de desenvolvimento

### 9.1 Sistema (Tauri)

Debian / Ubuntu:

```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

Fedora:

```bash
sudo dnf check-update
sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file \
  libappindicator-gtk3-devel librsvg2-devel libxdo-devel
sudo dnf group install "c-development"
```

### 9.2 Rust e Node

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup component add rustfmt clippy

curl -fsSL https://fnm.vercel.app/install | bash
fnm install --lts
```

### 9.3 Ferramentas do app (para testes)

Debian / Ubuntu:

```bash
sudo apt install ffmpeg qpdf tesseract-ocr tesseract-ocr-por tesseract-ocr-eng \
  libimage-exiftool-perl
sudo apt install libreoffice-writer libreoffice-calc libreoffice-impress \
  pandoc calibre ghostscript 7zip
```

Fedora:

```bash
sudo dnf install ffmpeg-free qpdf tesseract tesseract-langpack-por tesseract-langpack-eng \
  perl-Image-ExifTool
sudo dnf install libreoffice-writer libreoffice-calc libreoffice-impress \
  pandoc calibre ghostscript 7zip
```

PDFium: baixar `libpdfium.so` de pdfium-binaries para `src-tauri/resources/pdfium/` (script em `scripts/`).

Opcional, para abrir e controlar o app real em testes (tela virtual, WebDriver, cliques):

```bash
# Debian / Ubuntu
sudo apt install xvfb webkit2gtk-driver xdotool
# Fedora (confirmar o pacote do WebKitWebDriver com dnf provides '*/WebKitWebDriver')
sudo dnf install xorg-x11-server-Xvfb xdotool
# Ambos
cargo install tauri-driver --locked
```

A lista completa, por grupo, fica no README (seção "Programas necessários").

### 9.4 Observações por distro

- Fedora: `ffmpeg-free` não tem todos os encoders; o FFmpeg completo vem do RPM Fusion (`sudo dnf swap ffmpeg-free ffmpeg --allowerasing`).
- Fedora: o pacote `7zip` oficial não tem suporte a RAR.
- Debian: RAR fica em `7zip-rar` (repositório non-free).
- Confirmar nomes de pacotes com `apt-cache policy <pacote>` e `dnf info <pacote>` antes de cada release.

### 9.5 Windows

Build do Windows **pelo CI** (`windows-latest`). Para desenvolver numa máquina Windows: Microsoft C++ Build Tools ("Desktop development with C++"), WebView2, Rust (`x86_64-pc-windows-msvc`) e Node LTS.

---

## 10. Testes

| Nível | O que | Ferramenta |
|---|---|---|
| Unitário Rust | Nomes de saída, validação, montagem de argumentos, parsing de progresso | `cargo test` |
| Integração Rust | Cada operação com fixtures pequenas; verificar tipo, páginas, duração, dimensões, metadados | `cargo test`, fixtures em `src-tauri/tests/fixtures/` |
| Frontend | Stores, composables, formulários | `vitest` + `@vue/test-utils` |
| Manual | Instalação e uso | Ubuntu LTS, Fedora atual, Windows 11 |

- Testes que dependem de ferramenta opcional devem ser ignorados (não falhar) quando ela não estiver instalada.
- Fixtures geradas pelo autor ou com licença livre.

---

## 11. CI (GitHub Actions)

| Workflow | Gatilho | Runner | Passos |
|---|---|---|---|
| `ci.yml` | push e PR | `ubuntu-22.04` | deps de sistema, Rust, Node, ferramentas; fmt, clippy, cargo test, lint, typecheck, vitest |
| `release.yml` | tag `v*` | `ubuntu-22.04` | build `.deb` e `.rpm` com `tauri-apps/tauri-action` |
| `release.yml` | tag `v*` | `windows-latest` | `fetch-sidecars.ps1`, build `.msi` e `.exe` |

Build Linux no Ubuntu 22.04 para compatibilidade de glibc. Pacotes anexados a um GitHub Release.

---

## 12. Repositório

```
/
├── CLAUDE.md
├── LICENSE                   # GPL-3.0
├── README.md                 # descrição, aviso de IA, sem fins lucrativos, instalação, ferramentas opcionais
├── CONTRIBUTING.md           # contribuições sem compromisso
├── THIRD_PARTY_LICENSES.md
├── .github/workflows/
│   ├── ci.yml
│   └── release.yml
├── scripts/
│   ├── fetch-pdfium.sh
│   └── fetch-sidecars.ps1
├── src/
└── src-tauri/
```

`.gitignore` deve incluir `src-tauri/binaries/`, `src-tauri/resources/pdfium/`, `src-tauri/resources/windows/`, `target/`, `node_modules/`, `dist/`.

### Textos fixos

Aviso de IA (README):

```markdown
## Aviso sobre uso de IA

Este projeto foi desenvolvido com auxílio de ferramentas de inteligência artificial.
O código pode conter erros, falhas de segurança ou comportamentos inesperados.
Revise o código antes de usar com arquivos importantes e mantenha backups dos seus dados.
O software é fornecido sem garantias, conforme a licença GPL-3.0.
```

CONTRIBUTING.md:

```markdown
# Contribuindo

Este é um projeto pessoal, mantido nas horas livres do autor.

- Issues e pull requests são bem-vindos, mas não há compromisso de resposta, revisão ou prazo.
- Não há garantia de que uma contribuição será aceita.
- Você pode fazer um fork e seguir por conta própria, dentro dos termos da GPL-3.0.
```

---

## 13. Marcos

Cada marco termina com um release funcional.

- [x] **M0: Base**: repositório, LICENSE, README, CONTRIBUTING, `.gitignore`, CI, projeto Tauri + Vue rodando, CSP e capabilities
- [x] **M1: Infraestrutura**: resolvedor de ferramentas, fila, progresso, cancelamento, timeout, configurações, tela de ferramentas, drag and drop, i18n
- [x] **M2: Imagens**: Fase 1. Release `v0.1.0`
- [x] **M3: PDF**: Fase 2 com OCR. Release `v0.2.0`
- [ ] **M4: Vídeo e áudio**: Fase 3. Release `v0.3.0`
- [ ] **M5: Escritório**: Fase 4. Release `v0.4.0`
- [ ] **M6: Restante**: Fase 5. Release `v0.5.0`
- [ ] **M7: Polimento**: revisão de erros e mensagens, `THIRD_PARTY_LICENSES.md`, i18n completo. Release `v1.0.0`

---

## 14. Decisões

### Pendentes

Nenhuma no momento. Perguntar ao autor antes de assumir qualquer decisão nova de produto, licença ou distribuição.

### Tomadas

- **Idiomas da interface**: pt-BR e inglês desde o início. Todo texto novo entra em `pt-BR.json` e `en.json`.
- **Biblioteca de componentes de UI**: nenhuma. CSS próprio, com variáveis CSS e tema claro/escuro.
- **Nome do app**: `Easy Converter`, identificador `com.wvmj.easyconverter` (decidido antes do `v0.1.0`; trocar depois faz os usuários perderem as configurações).
- **Ghostscript**: mantido como ferramenta opcional para compressão forte de PDF (processo separado, nunca distribuído).
- **Ferramentas obrigatórias desde o `v0.1.0`**: FFmpeg, qpdf, Tesseract e PDFium já vão nos pacotes, mesmo antes das fases que os usam.
- **Versões fixas dos binários**: definidas em `scripts/fetch-pdfium.sh` e `scripts/fetch-sidecars.ps1`, com SHA-256. Ao atualizar uma versão, atualizar também `THIRD_PARTY_LICENSES.md`.
- **Release**: o `release.yml` cria um rascunho no GitHub; o autor revisa e publica manualmente.
- **Crates aprovadas no M3**: `lopdf` (só para **escrever** PDFs a partir de imagens; nunca para ler PDFs do usuário) e `pdfium-render` (só dentro do worker).
- **PDFium em processo separado**: o próprio executável do app, iniciado com `--pdfium-worker <pedido.json>` (`src/pdf_worker.rs`). Um PDF que trave ou derrube o PDFium não derruba o app, e cancelamento e timeout matam o processo na hora. O pedido vai num arquivo na pasta temporária da tarefa, nunca na linha de comando.
- **Senhas fora da linha de comando**: senhas de PDF vão para o qpdf por arquivo de argumentos (`@arquivo`) ou `--password-file`, criados só legíveis pelo usuário na pasta temporária da tarefa. Os parâmetros com senha têm `Debug` que oculta o valor.
- **OCR sem perda**: o Tesseract gera páginas só com texto (`textonly_pdf=1`), que o qpdf sobrepõe às páginas originais (`--overlay`); o original não é recodificado.
- **Repositório**: https://github.com/wvmj-dev/easy-converter. O autor faz push e cria as tags.
- **Crates aprovadas no M1**: `tokio` (processos filhos, timeout, sincronização), `thiserror` (`AppError`), `serde`, `serde_json`, `ts-rs` (tipos TypeScript).

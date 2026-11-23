<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { version } from '../../package.json'

type Group = 'app' | 'bundled' | 'optional'

interface Credit {
  name: string
  license: string
  source: string
}

// Names and licenses are proper nouns, not interface text. Exact versions of the
// distributed binaries are listed in THIRD_PARTY_LICENSES.md.
const CREDITS: Record<Group, Credit[]> = {
  app: [
    { name: 'Tauri', license: 'MIT / Apache-2.0', source: 'github.com/tauri-apps/tauri' },
    { name: 'Vue', license: 'MIT', source: 'github.com/vuejs/core' },
    { name: 'Vue Router', license: 'MIT', source: 'github.com/vuejs/router' },
    { name: 'Pinia', license: 'MIT', source: 'github.com/vuejs/pinia' },
    { name: 'Vue I18n', license: 'MIT', source: 'github.com/intlify/vue-i18n' },
    { name: 'image (Rust)', license: 'MIT / Apache-2.0', source: 'github.com/image-rs/image' },
    { name: 'lopdf', license: 'MIT', source: 'github.com/J-F-Liu/lopdf' },
    { name: 'pdfium-render', license: 'MIT / Apache-2.0', source: 'github.com/ajrcarey/pdfium-render' },
    { name: 'csv (Rust)', license: 'MIT / Unlicense', source: 'github.com/BurntSushi/rust-csv' },
    { name: 'zip (Rust)', license: 'MIT', source: 'github.com/zip-rs/zip2' },
    { name: 'tar (Rust)', license: 'MIT / Apache-2.0', source: 'github.com/composefs/tar-rs' },
    { name: 'flate2', license: 'MIT / Apache-2.0', source: 'github.com/rust-lang/flate2-rs' },
    { name: 'sevenz-rust2', license: 'Apache-2.0', source: 'github.com/hasenbanck/sevenz-rust' },
    { name: 'Tokio', license: 'MIT', source: 'github.com/tokio-rs/tokio' },
    { name: 'Serde', license: 'MIT / Apache-2.0', source: 'github.com/serde-rs/serde' },
  ],
  bundled: [
    { name: 'FFmpeg', license: 'GPL-3.0', source: 'ffmpeg.org' },
    { name: 'qpdf', license: 'Apache-2.0', source: 'github.com/qpdf/qpdf' },
    { name: 'Tesseract OCR', license: 'Apache-2.0', source: 'github.com/tesseract-ocr/tesseract' },
    { name: 'PDFium', license: 'BSD-3-Clause', source: 'pdfium.googlesource.com/pdfium' },
  ],
  optional: [
    { name: 'LibreOffice', license: 'MPL-2.0', source: 'libreoffice.org' },
    { name: 'Pandoc', license: 'GPL-2.0+', source: 'pandoc.org' },
    { name: 'Calibre', license: 'GPL-3.0', source: 'calibre-ebook.com' },
    { name: 'Ghostscript', license: 'AGPL-3.0', source: 'ghostscript.com' },
    { name: '7-Zip', license: 'LGPL-2.1 + unRAR', source: '7-zip.org' },
  ],
}
const GROUPS: Group[] = ['app', 'bundled', 'optional']

const { t } = useI18n()
</script>

<template>
  <div class="page">
    <header class="page__header">
      <h1 class="page__title">
        {{ t('app.name') }}
      </h1>
      <p class="page__subtitle">
        {{ t('about.version', { version }) }}
      </p>
    </header>

    <section class="card about__section">
      <p>{{ t('about.description') }}</p>
      <p>{{ t('about.privacy') }}</p>
    </section>

    <section class="card about__section">
      <h2 class="section-title">
        {{ t('about.license.title') }}
      </h2>
      <p>{{ t('about.license.text') }}</p>
    </section>

    <section
      class="card about__section"
      role="note"
    >
      <h2 class="section-title">
        {{ t('about.ai.title') }}
      </h2>
      <p>{{ t('about.ai.text') }}</p>
    </section>

    <section class="card about__section">
      <h2 class="section-title">
        {{ t('about.credits.title') }}
      </h2>
      <template
        v-for="group in GROUPS"
        :key="group"
      >
        <h3 class="about__group">
          {{ t(`about.credits.${group}`) }}
        </h3>
        <ul class="about__credits">
          <li
            v-for="credit in CREDITS[group]"
            :key="credit.name"
            class="about__credit"
          >
            <span class="about__name">{{ credit.name }}</span>
            <span class="about__license">{{ credit.license }}</span>
            <span class="about__source">{{ credit.source }}</span>
          </li>
        </ul>
      </template>
      <p class="field__help">
        {{ t('about.credits.details') }}
      </p>
    </section>
  </div>
</template>

<style scoped>
.about__section p {
  margin: 0;
}

.about__section {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.about__group {
  margin: var(--space-2) 0 0;
  font-size: 0.875rem;
  color: var(--color-muted);
}

.about__credits {
  margin: 0;
  padding: 0;
  list-style: none;
}

.about__credit {
  display: grid;
  grid-template-columns: minmax(9rem, 1fr) minmax(8rem, 1fr) 2fr;
  gap: var(--space-2);
  padding: var(--space-1) 0;
  border-bottom: 1px solid var(--color-border);
  font-size: 0.875rem;
}

.about__name {
  font-weight: 600;
}

.about__license,
.about__source {
  color: var(--color-muted);
  overflow-wrap: anywhere;
  user-select: text;
}
</style>

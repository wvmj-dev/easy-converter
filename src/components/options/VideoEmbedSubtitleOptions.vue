<script setup lang="ts">
import { open } from '@tauri-apps/plugin-dialog'
import { useI18n } from 'vue-i18n'
import { SUBTITLE_EXTENSIONS, SUBTITLE_LANGUAGES } from '../../operations/subtitle'
import type { VideoOptions } from '../../operations/video'
import { baseName } from '../../utils/format'

const options = defineModel<VideoOptions['embedSubtitle']>({ required: true })
const { t } = useI18n()

async function chooseSubtitle() {
  const selected = await open({
    multiple: false,
    directory: false,
    filters: [{ name: t('video.embedSubtitle.filter'), extensions: SUBTITLE_EXTENSIONS }],
  })
  if (typeof selected === 'string') options.value.subtitle = selected
}
</script>

<template>
  <div class="options">
    <div class="field">
      <button
        type="button"
        class="btn"
        @click="chooseSubtitle"
      >
        {{ t('video.embedSubtitle.choose') }}
      </button>
      <p class="field__help">
        {{ options.subtitle ? baseName(options.subtitle) : t('video.embedSubtitle.none') }}
      </p>
    </div>
    <div class="field">
      <label
        class="field__label"
        for="subtitle-language"
      >{{ t('video.embedSubtitle.language') }}</label>
      <select
        id="subtitle-language"
        v-model="options.language"
        class="input"
      >
        <option value="">
          {{ t('video.embedSubtitle.languages.none') }}
        </option>
        <option
          v-for="code in SUBTITLE_LANGUAGES"
          :key="code"
          :value="code"
        >
          {{ t(`video.embedSubtitle.languages.${code}`) }}
        </option>
      </select>
      <p class="field__help">
        {{ t('video.embedSubtitle.help') }}
      </p>
    </div>
  </div>
</template>

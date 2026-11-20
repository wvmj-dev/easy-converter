<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import type { PdfOptions } from '../../operations/pdf'

defineProps<{ languages: string[] }>()
const options = defineModel<PdfOptions['ocr']>({ required: true })
const { t, te } = useI18n()
const languageName = (code: string) => (te(`pdf.ocr.names.${code}`) ? t(`pdf.ocr.names.${code}`) : code)
</script>

<template>
  <div class="options">
    <fieldset class="field">
      <legend class="field__label">
        {{ t('pdf.ocr.languages') }}
      </legend>
      <label
        v-for="code in languages"
        :key="code"
        class="radio"
      >
        <input
          v-model="options.languages"
          type="checkbox"
          :value="code"
        >
        {{ languageName(code) }}
      </label>
    </fieldset>
    <p class="field__help">
      {{ t('pdf.ocr.help') }}
    </p>
  </div>
</template>

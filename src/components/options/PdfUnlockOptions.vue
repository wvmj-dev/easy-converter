<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import type { PdfOptions } from '../../operations/pdf'

const options = defineModel<PdfOptions['unlock']>({ required: true })
const { t } = useI18n()
</script>

<template>
  <div class="field">
    <label
      class="field__label"
      for="unlock-password"
    >{{ t('pdf.unlock.password') }}</label>
    <input
      id="unlock-password"
      v-model="options.password"
      class="input"
      type="password"
      autocomplete="off"
    >
    <p class="field__help">
      {{ t('pdf.unlock.help') }}
    </p>
  </div>
</template>

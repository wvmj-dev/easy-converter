<script setup lang="ts">
import { useI18n } from 'vue-i18n'

defineProps<{ id: string; label: string; help?: string }>()
const pages = defineModel<string>({ required: true })
const { t } = useI18n()
</script>

<template>
  <div class="field">
    <label
      class="field__label"
      :for="id"
    >{{ label }}</label>
    <input
      :id="id"
      v-model.trim="pages"
      class="input"
      type="text"
      inputmode="numeric"
      autocomplete="off"
      spellcheck="false"
      :placeholder="t('pdf.pages.placeholder')"
    >
    <p class="field__help">
      {{ help ?? t('pdf.pages.help') }}
    </p>
  </div>
</template>

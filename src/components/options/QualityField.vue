<script setup lang="ts">
import { useI18n } from 'vue-i18n'

defineProps<{ id: string; help: string }>()
const quality = defineModel<number>({ required: true })
const { t } = useI18n()
</script>

<template>
  <div class="field">
    <label
      class="field__label"
      :for="id"
    >{{ t('image.quality.label') }}</label>
    <div class="field__row">
      <input
        :id="id"
        v-model.number="quality"
        class="range"
        type="range"
        min="1"
        max="100"
        step="1"
      >
      <input
        v-model.number="quality"
        class="input input--number"
        type="number"
        min="1"
        max="100"
        step="1"
        :aria-label="t('image.quality.label')"
      >
    </div>
    <p class="field__help">
      {{ help }}
    </p>
  </div>
</template>

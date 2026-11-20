<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import type { RangeText } from '../../operations/media'

defineProps<{ id: string; help?: string }>()
const range = defineModel<RangeText>({ required: true })
const { t } = useI18n()
</script>

<template>
  <fieldset class="field">
    <legend class="field__label">
      {{ t('media.time.label') }}
    </legend>
    <div class="time-range">
      <label :for="`${id}-start`">{{ t('media.time.start') }}</label>
      <input
        :id="`${id}-start`"
        v-model.trim="range.start"
        class="input time-range__input"
        type="text"
        inputmode="decimal"
        :placeholder="t('media.time.startPlaceholder')"
      >
      <label :for="`${id}-end`">{{ t('media.time.end') }}</label>
      <input
        :id="`${id}-end`"
        v-model.trim="range.end"
        class="input time-range__input"
        type="text"
        inputmode="decimal"
        :placeholder="t('media.time.endPlaceholder')"
      >
    </div>
    <p class="field__help">
      {{ help ?? t('media.time.help') }}
    </p>
  </fieldset>
</template>

<style scoped>
.time-range {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--space-2) var(--space-3);
}

.time-range__input {
  width: 9rem;
}
</style>

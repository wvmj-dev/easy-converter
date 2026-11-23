<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { SubtitleOptions } from '../../operations/subtitle'
import ChoiceField from './ChoiceField.vue'

const options = defineModel<SubtitleOptions['shift']>({ required: true })
const { t } = useI18n()
const directions = computed(() =>
  (['later', 'earlier'] as const).map((value) => ({ value, label: t(`subtitle.shift.directions.${value}`) })),
)
</script>

<template>
  <div class="options">
    <div class="field">
      <label
        class="field__label"
        for="subtitle-offset"
      >{{ t('subtitle.shift.offset') }}</label>
      <input
        id="subtitle-offset"
        v-model="options.offset"
        class="input"
        type="text"
        inputmode="decimal"
        :placeholder="t('subtitle.shift.placeholder')"
      >
      <p class="field__help">
        {{ t('subtitle.shift.help') }}
      </p>
    </div>
    <ChoiceField
      v-model="options.direction"
      :legend="t('subtitle.shift.direction')"
      name="subtitle-direction"
      :choices="directions"
    />
  </div>
</template>

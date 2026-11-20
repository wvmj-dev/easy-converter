<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { PdfOptions } from '../../operations/pdf'
import ChoiceField from './ChoiceField.vue'
import PagesField from './PagesField.vue'

const options = defineModel<PdfOptions['split']>({ required: true })
const { t } = useI18n()
const modes = computed(() =>
  (['everyPage', 'every', 'ranges'] as const).map((value) => ({
    value,
    label: t(`pdf.split.modes.${value}`),
  })),
)
</script>

<template>
  <div class="options">
    <ChoiceField
      v-model="options.mode"
      :legend="t('pdf.split.label')"
      name="split-mode"
      :choices="modes"
    />
    <div
      v-if="options.mode === 'every'"
      class="field"
    >
      <label
        class="field__label"
        for="split-every"
      >{{ t('pdf.split.every') }}</label>
      <input
        id="split-every"
        v-model.number="options.every"
        class="input input--number"
        type="number"
        min="1"
      >
    </div>
    <PagesField
      v-else-if="options.mode === 'ranges'"
      id="split-ranges"
      v-model="options.ranges"
      :label="t('pdf.split.ranges')"
      :help="t('pdf.split.rangesHelp')"
    />
  </div>
</template>

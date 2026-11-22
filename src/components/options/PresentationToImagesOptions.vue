<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { PDF_LIMITS } from '../../operations/pdf'
import type { PresentationOptions } from '../../operations/presentation'
import type { PdfImageFormat } from '../../types/PdfImageFormat'
import ChoiceField from './ChoiceField.vue'
import QualityField from './QualityField.vue'

const options = defineModel<PresentationOptions['toImages']>({ required: true })
const { t } = useI18n()
const formats = computed(() =>
  (['png', 'jpeg'] as PdfImageFormat[]).map((value) => ({
    value,
    label: t(`image.format.names.${value}`),
  })),
)
</script>

<template>
  <div class="options">
    <ChoiceField
      v-model="options.format"
      :legend="t('image.format.label')"
      name="slide-image-format"
      :choices="formats"
    />
    <div class="field">
      <label
        class="field__label"
        for="slide-dpi"
      >{{ t('pdf.toImages.dpi') }}</label>
      <input
        id="slide-dpi"
        v-model.number="options.dpi"
        class="input input--number"
        type="number"
        :min="PDF_LIMITS.minDpi"
        :max="PDF_LIMITS.maxDpi"
        step="1"
      >
      <p class="field__help">
        {{ t('presentation.toImages.dpiHelp') }}
      </p>
    </div>
    <QualityField
      v-if="options.format === 'jpeg'"
      id="slide-image-quality"
      v-model="options.quality"
      :help="t('image.quality.help')"
    />
  </div>
</template>

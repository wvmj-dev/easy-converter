<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { PDF_LIMITS, type PdfOptions } from '../../operations/pdf'
import type { PdfImageFormat } from '../../types/PdfImageFormat'
import ChoiceField from './ChoiceField.vue'
import PageScopeField from './PageScopeField.vue'
import QualityField from './QualityField.vue'

const options = defineModel<PdfOptions['toImages']>({ required: true })
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
      name="pdf-image-format"
      :choices="formats"
    />
    <div class="field">
      <label
        class="field__label"
        for="pdf-dpi"
      >{{ t('pdf.toImages.dpi') }}</label>
      <input
        id="pdf-dpi"
        v-model.number="options.dpi"
        class="input input--number"
        type="number"
        :min="PDF_LIMITS.minDpi"
        :max="PDF_LIMITS.maxDpi"
        step="1"
      >
      <p class="field__help">
        {{ t('pdf.toImages.dpiHelp') }}
      </p>
    </div>
    <QualityField
      v-if="options.format === 'jpeg'"
      id="pdf-image-quality"
      v-model="options.quality"
      :help="t('image.quality.help')"
    />
    <PageScopeField
      id="to-images"
      v-model:scope="options.scope"
      v-model:pages="options.pages"
    />
  </div>
</template>

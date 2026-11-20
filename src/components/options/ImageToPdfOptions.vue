<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { IMAGE_LIMITS, type ImageOptions } from '../../operations/image'
import type { PdfPageSize } from '../../types/PdfPageSize'
import ChoiceField from './ChoiceField.vue'

const options = defineModel<ImageOptions['toPdf']>({ required: true })
const { t } = useI18n()
const sizes = computed(() =>
  (['a4', 'letter', 'image'] as PdfPageSize[]).map((value) => ({
    value,
    label: t(`image.toPdf.sizes.${value}`),
  })),
)
</script>

<template>
  <div class="options">
    <ChoiceField
      v-model="options.pageSize"
      :legend="t('image.toPdf.pageSize')"
      name="pdf-page-size"
      :choices="sizes"
    />
    <div class="field">
      <label
        class="field__label"
        for="pdf-margin"
      >{{ t('image.toPdf.margin') }}</label>
      <input
        id="pdf-margin"
        v-model.number="options.marginMm"
        class="input input--number"
        type="number"
        min="0"
        :max="IMAGE_LIMITS.maxPdfMarginMm"
      >
    </div>
    <p class="field__help">
      {{ t('image.toPdf.help') }}
    </p>
  </div>
</template>

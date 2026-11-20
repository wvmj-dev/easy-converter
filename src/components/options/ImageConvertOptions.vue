<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import type { ImageOptions } from '../../operations/image'
import FormatSelect from './FormatSelect.vue'
import QualityField from './QualityField.vue'

const options = defineModel<ImageOptions['convert']>({ required: true })
const { t } = useI18n()
</script>

<template>
  <div class="options">
    <FormatSelect
      id="convert-format"
      v-model="options.format"
    />
    <QualityField
      v-if="options.format === 'jpeg'"
      id="convert-quality"
      v-model="options.quality"
      :help="t('image.quality.help')"
    />
    <p class="field__help">
      {{ t('image.convert.metadataNote') }}
    </p>
  </div>
</template>

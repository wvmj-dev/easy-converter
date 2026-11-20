<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import type { ImageOptions } from '../../operations/image'
import QualityField from './QualityField.vue'

const options = defineModel<ImageOptions['compress']>({ required: true })
const { t } = useI18n()
</script>

<template>
  <div class="options">
    <QualityField
      id="compress-quality"
      v-model="options.quality"
      :help="t('image.compress.qualityHelp')"
    />
    <p class="field__help">
      {{ t('image.compress.help') }}
    </p>
  </div>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { OUTPUT_FORMATS } from '../../operations/image'
import type { ImageOutputFormat } from '../../types/ImageOutputFormat'

defineProps<{ id: string }>()
const format = defineModel<ImageOutputFormat>({ required: true })
const { t } = useI18n()
</script>

<template>
  <div class="field">
    <label
      class="field__label"
      :for="id"
    >{{ t('image.format.label') }}</label>
    <select
      :id="id"
      v-model="format"
      class="input"
    >
      <option
        v-for="option in OUTPUT_FORMATS"
        :key="option"
        :value="option"
      >
        {{ t(`image.format.names.${option}`) }}
      </option>
    </select>
    <p class="field__help">
      {{ t(`image.format.hints.${format}`) }}
    </p>
  </div>
</template>

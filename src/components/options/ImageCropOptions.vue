<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { IMAGE_LIMITS, type CropArea } from '../../operations/image'
import type { ImageInfo } from '../../types/ImageInfo'

defineProps<{ reference: ImageInfo | null; multiple: boolean }>()
const emit = defineEmits<{ reset: [] }>()
const area = defineModel<CropArea>({ required: true })
const { t } = useI18n()
const FIELDS = ['x', 'y', 'width', 'height'] as const
</script>

<template>
  <div class="options">
    <p
      v-if="reference"
      class="field__row field__help"
    >
      {{ t('image.crop.reference', { width: reference.width, height: reference.height }) }}
      <button
        type="button"
        class="btn btn--ghost btn--small"
        @click="emit('reset')"
      >
        {{ t('image.crop.whole') }}
      </button>
    </p>
    <div class="form-grid">
      <div
        v-for="field in FIELDS"
        :key="field"
        class="field"
      >
        <label
          class="field__label"
          :for="`crop-${field}`"
        >{{ t(`image.crop.${field}`) }}</label>
        <input
          :id="`crop-${field}`"
          v-model.number="area[field]"
          class="input input--number"
          type="number"
          :min="field === 'x' || field === 'y' ? 0 : 1"
          :max="IMAGE_LIMITS.maxDimension"
        >
      </div>
    </div>
    <p
      v-if="multiple"
      class="field__help"
    >
      {{ t('image.crop.sameArea') }}
    </p>
  </div>
</template>

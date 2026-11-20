<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { IMAGE_LIMITS } from '../../operations/image'
import type { ResizeMode } from '../../types/ResizeMode'

const resize = defineModel<ResizeMode>({ required: true })
const { t } = useI18n()
const MODES: ResizeMode['mode'][] = ['fit', 'exact', 'percent']

const mode = computed({
  get: () => resize.value.mode,
  set: (value: ResizeMode['mode']) => {
    const current = resize.value
    if (value === 'percent') {
      resize.value = { mode: 'percent', percent: 50 }
    } else {
      const size = current.mode === 'percent' ? { width: 1920, height: 1080 } : current
      resize.value = { mode: value, width: size.width, height: size.height }
    }
  },
})
</script>

<template>
  <div class="options">
    <fieldset class="field">
      <legend class="field__label">
        {{ t('image.resize.mode') }}
      </legend>
      <label
        v-for="option in MODES"
        :key="option"
        class="radio"
      >
        <input
          v-model="mode"
          type="radio"
          name="resize-mode"
          :value="option"
        >
        <span>
          {{ t(`image.resize.modes.${option}`) }}
          <span class="field__help">— {{ t(`image.resize.help.${option}`) }}</span>
        </span>
      </label>
    </fieldset>

    <div
      v-if="resize.mode !== 'percent'"
      class="form-grid"
    >
      <div class="field">
        <label
          class="field__label"
          for="resize-width"
        >{{ t('image.resize.width') }}</label>
        <input
          id="resize-width"
          v-model.number="resize.width"
          class="input input--number"
          type="number"
          min="1"
          :max="IMAGE_LIMITS.maxDimension"
        >
      </div>
      <div class="field">
        <label
          class="field__label"
          for="resize-height"
        >{{ t('image.resize.height') }}</label>
        <input
          id="resize-height"
          v-model.number="resize.height"
          class="input input--number"
          type="number"
          min="1"
          :max="IMAGE_LIMITS.maxDimension"
        >
      </div>
    </div>
    <div
      v-else
      class="field"
    >
      <label
        class="field__label"
        for="resize-percent"
      >{{ t('image.resize.percent') }}</label>
      <input
        id="resize-percent"
        v-model.number="resize.percent"
        class="input input--number"
        type="number"
        min="1"
        :max="IMAGE_LIMITS.maxPercent"
      >
    </div>
  </div>
</template>

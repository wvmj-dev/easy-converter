<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { IMAGE_LIMITS, type ImageOptions } from '../../operations/image'
import type { MergeDirection } from '../../types/MergeDirection'
import ChoiceField from './ChoiceField.vue'
import FormatSelect from './FormatSelect.vue'

const options = defineModel<ImageOptions['merge']>({ required: true })
const { t } = useI18n()
const directions = computed(() =>
  (['horizontal', 'vertical'] as MergeDirection[]).map((value) => ({
    value,
    label: t(`image.merge.${value}`),
  })),
)
</script>

<template>
  <div class="options">
    <ChoiceField
      v-model="options.direction"
      :legend="t('image.merge.direction')"
      name="merge-direction"
      :choices="directions"
    />
    <label class="radio">
      <input
        v-model="options.matchSize"
        type="checkbox"
      >
      {{ t('image.merge.matchSize') }}
    </label>
    <div class="field">
      <label
        class="field__label"
        for="merge-spacing"
      >{{ t('image.merge.spacing') }}</label>
      <input
        id="merge-spacing"
        v-model.number="options.spacing"
        class="input input--number"
        type="number"
        min="0"
        :max="IMAGE_LIMITS.maxSpacing"
      >
    </div>
    <FormatSelect
      id="merge-format"
      v-model="options.format"
    />
    <p class="field__help">
      {{ t('image.merge.order') }}
    </p>
  </div>
</template>

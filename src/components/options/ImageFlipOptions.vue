<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { ImageOptions } from '../../operations/image'
import type { FlipAxis } from '../../types/FlipAxis'
import ChoiceField from './ChoiceField.vue'

const options = defineModel<ImageOptions['flip']>({ required: true })
const { t } = useI18n()
const choices = computed(() =>
  (['horizontal', 'vertical'] as FlipAxis[]).map((value) => ({
    value,
    label: t(`image.flip.${value}`),
  })),
)
</script>

<template>
  <ChoiceField
    v-model="options.axis"
    :legend="t('image.flip.label')"
    name="flip-axis"
    :choices="choices"
  />
</template>

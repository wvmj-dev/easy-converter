<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { ImageOptions } from '../../operations/image'
import type { Rotation } from '../../types/Rotation'
import ChoiceField from './ChoiceField.vue'

const options = defineModel<ImageOptions['rotate']>({ required: true })
const { t } = useI18n()
const choices = computed(() =>
  (['cw90', 'cw180', 'cw270'] as Rotation[]).map((value) => ({
    value,
    label: t(`image.rotate.${value}`),
  })),
)
</script>

<template>
  <ChoiceField
    v-model="options.rotation"
    :legend="t('image.rotate.label')"
    name="rotation"
    :choices="choices"
  />
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { PdfOptions } from '../../operations/pdf'
import type { Rotation } from '../../types/Rotation'
import ChoiceField from './ChoiceField.vue'
import PageScopeField from './PageScopeField.vue'

const options = defineModel<PdfOptions['rotate']>({ required: true })
const { t } = useI18n()
const rotations = computed(() =>
  (['cw90', 'cw180', 'cw270'] as Rotation[]).map((value) => ({
    value,
    label: t(`image.rotate.${value}`),
  })),
)
</script>

<template>
  <div class="options">
    <ChoiceField
      v-model="options.rotation"
      :legend="t('image.rotate.label')"
      name="pdf-rotation"
      :choices="rotations"
    />
    <PageScopeField
      id="rotate"
      v-model:scope="options.scope"
      v-model:pages="options.pages"
    />
  </div>
</template>

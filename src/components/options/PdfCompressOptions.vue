<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { PdfOptions } from '../../operations/pdf'
import type { PdfCompressLevel } from '../../types/PdfCompressLevel'
import ChoiceField from './ChoiceField.vue'

defineProps<{ strongAvailable: boolean }>()
const options = defineModel<PdfOptions['compress']>({ required: true })
const { t } = useI18n()
const levels = computed(() =>
  (['standard', 'strong'] as PdfCompressLevel[]).map((value) => ({
    value,
    label: t(`pdf.compress.levels.${value}`),
  })),
)
</script>

<template>
  <div class="options">
    <ChoiceField
      v-model="options.level"
      :legend="t('pdf.compress.label')"
      name="pdf-compress-level"
      :choices="levels"
    />
    <p class="field__help">
      {{ t(`pdf.compress.hints.${options.level}`) }}
    </p>
    <p
      v-if="!strongAvailable"
      class="field__help"
    >
      {{ t('pdf.compress.installGhostscript') }}
    </p>
  </div>
</template>

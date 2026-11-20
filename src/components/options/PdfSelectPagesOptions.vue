<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import type { PdfOptions } from '../../operations/pdf'
import PagesField from './PagesField.vue'

const options = defineModel<PdfOptions['selectPages']>({ required: true })
const { t } = useI18n()
</script>

<template>
  <PagesField
    id="select-pages"
    v-model="options.pages"
    :label="t('pdf.selectPages.label')"
    :help="t('pdf.selectPages.help')"
  />
</template>

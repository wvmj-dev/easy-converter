<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { PageScope } from '../../operations/pdf'
import ChoiceField from './ChoiceField.vue'
import PagesField from './PagesField.vue'

const props = defineProps<{ id: string }>()
const scope = defineModel<PageScope>('scope', { required: true })
const pages = defineModel<string>('pages', { required: true })
const { t } = useI18n()
const choices = computed(() =>
  (['all', 'some'] as PageScope[]).map((value) => ({ value, label: t(`pdf.scope.${value}`) })),
)
</script>

<template>
  <ChoiceField
    v-model="scope"
    :legend="t('pdf.scope.label')"
    :name="`${props.id}-scope`"
    :choices="choices"
  />
  <PagesField
    v-if="scope === 'some'"
    :id="`${props.id}-pages`"
    v-model="pages"
    :label="t('pdf.pages.label')"
  />
</template>

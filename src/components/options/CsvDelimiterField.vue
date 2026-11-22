<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { CSV_DELIMITERS } from '../../operations/spreadsheet'
import type { CsvDelimiter } from '../../types/CsvDelimiter'

const props = defineProps<{ id: string; allowAuto?: boolean }>()
const delimiter = defineModel<CsvDelimiter | 'auto'>({ required: true })
const { t } = useI18n()
const choices = computed(() => [...(props.allowAuto ? (['auto'] as const) : []), ...CSV_DELIMITERS])
</script>

<template>
  <div class="field">
    <label
      class="field__label"
      :for="id"
    >{{ t('spreadsheet.delimiter.label') }}</label>
    <select
      :id="id"
      v-model="delimiter"
      class="input"
    >
      <option
        v-for="choice in choices"
        :key="choice"
        :value="choice"
      >
        {{ t(`spreadsheet.delimiter.names.${choice}`) }}
      </option>
    </select>
  </div>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import type { SpreadsheetOptions } from '../../operations/spreadsheet'
import CsvDelimiterField from './CsvDelimiterField.vue'

const options = defineModel<SpreadsheetOptions['jsonToCsv']>({ required: true })
const { t } = useI18n()
</script>

<template>
  <div class="options">
    <CsvDelimiterField
      id="json-to-csv-delimiter"
      v-model="options.delimiter"
    />
    <fieldset class="field">
      <legend class="field__label">
        {{ t('spreadsheet.jsonToCsv.legend') }}
      </legend>
      <label class="radio">
        <input
          v-model="options.bom"
          type="checkbox"
        >
        {{ t('spreadsheet.jsonToCsv.bom') }}
      </label>
      <label class="radio">
        <input
          v-model="options.escapeFormulas"
          type="checkbox"
        >
        {{ t('spreadsheet.jsonToCsv.escapeFormulas') }}
      </label>
      <p class="field__help">
        {{ t('spreadsheet.jsonToCsv.help') }}
      </p>
    </fieldset>
  </div>
</template>

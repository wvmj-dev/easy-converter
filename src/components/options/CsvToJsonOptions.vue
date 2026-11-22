<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import type { SpreadsheetOptions } from '../../operations/spreadsheet'
import CsvDelimiterField from './CsvDelimiterField.vue'

const options = defineModel<SpreadsheetOptions['csvToJson']>({ required: true })
const { t } = useI18n()
</script>

<template>
  <div class="options">
    <CsvDelimiterField
      id="csv-to-json-delimiter"
      v-model="options.delimiter"
      allow-auto
    />
    <fieldset class="field">
      <legend class="field__label">
        {{ t('spreadsheet.csvToJson.legend') }}
      </legend>
      <label class="radio">
        <input
          v-model="options.header"
          type="checkbox"
        >
        {{ t('spreadsheet.csvToJson.header') }}
      </label>
      <label class="radio">
        <input
          v-model="options.typed"
          type="checkbox"
        >
        {{ t('spreadsheet.csvToJson.typed') }}
      </label>
      <p class="field__help">
        {{ t('spreadsheet.csvToJson.help') }}
      </p>
    </fieldset>
  </div>
</template>

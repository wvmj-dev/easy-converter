<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import type { PdfOptions } from '../../operations/pdf'

const options = defineModel<PdfOptions['protect']>({ required: true })
const { t } = useI18n()
</script>

<template>
  <div class="options">
    <div class="field">
      <label
        class="field__label"
        for="protect-password"
      >{{ t('pdf.protect.password') }}</label>
      <input
        id="protect-password"
        v-model="options.password"
        class="input"
        type="password"
        autocomplete="new-password"
      >
    </div>
    <div class="field">
      <label
        class="field__label"
        for="protect-confirm"
      >{{ t('pdf.protect.confirm') }}</label>
      <input
        id="protect-confirm"
        v-model="options.confirm"
        class="input"
        type="password"
        autocomplete="new-password"
      >
    </div>
    <fieldset class="field">
      <legend class="field__label">
        {{ t('pdf.protect.restrictions') }}
      </legend>
      <label class="radio">
        <input
          v-model="options.noPrint"
          type="checkbox"
        >
        {{ t('pdf.protect.noPrint') }}
      </label>
      <label class="radio">
        <input
          v-model="options.noCopy"
          type="checkbox"
        >
        {{ t('pdf.protect.noCopy') }}
      </label>
      <label class="radio">
        <input
          v-model="options.noEdit"
          type="checkbox"
        >
        {{ t('pdf.protect.noEdit') }}
      </label>
    </fieldset>
    <div class="field">
      <label
        class="field__label"
        for="protect-owner"
      >{{ t('pdf.protect.owner') }}</label>
      <input
        id="protect-owner"
        v-model="options.ownerPassword"
        class="input"
        type="password"
        autocomplete="new-password"
      >
      <p class="field__help">
        {{ t('pdf.protect.ownerHelp') }}
      </p>
    </div>
    <p class="field__help">
      {{ t('pdf.protect.help') }}
    </p>
  </div>
</template>

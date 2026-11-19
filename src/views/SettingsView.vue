<script setup lang="ts">
import { computed, onMounted, ref, toRaw, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { open } from '@tauri-apps/plugin-dialog'
import { useSettingsStore } from '../stores/settings'
import type { JobCategory } from '../types/JobCategory'
import type { Language } from '../types/Language'
import type { Settings } from '../types/Settings'
import { errorMessage } from '../utils/errors'

const CATEGORIES: JobCategory[] = [
  'image',
  'pdf',
  'video',
  'audio',
  'document',
  'spreadsheet',
  'presentation',
  'subtitle',
  'ebook',
  'archive',
]
const LANGUAGES: { value: Language; key: string }[] = [
  { value: 'auto', key: 'settings.language.auto' },
  { value: 'pt-BR', key: 'settings.language.ptBR' },
  { value: 'en', key: 'settings.language.en' },
]
// Mirrors the limits validated by the backend.
const MAX_CONCURRENCY = 64
const MAX_VIDEO_CONCURRENCY = 4
const MAX_TIMEOUT_MINUTES = 24 * 60

const { t } = useI18n()
const store = useSettingsStore()
const form = ref<Settings | null>(null)
const saving = ref(false)
const error = ref<string | null>(null)
const saved = ref(false)

function reset() {
  // structuredClone cannot copy reactive proxies; clone the raw object instead.
  form.value = store.settings ? structuredClone(toRaw(store.settings)) : null
}

onMounted(async () => {
  if (!store.settings) {
    try {
      await store.load()
    } catch (err) {
      error.value = errorMessage(t, err)
    }
  }
  reset()
})
watch(() => store.settings, reset)

const outputMode = computed({
  get: () => form.value?.output.mode ?? 'sameFolder',
  set: (mode) => {
    if (!form.value) return
    form.value.output = mode === 'folder' ? { mode: 'folder', path: '' } : { mode: 'sameFolder' }
  },
})
const outputFolder = computed(() =>
  form.value?.output.mode === 'folder' ? form.value.output.path : '',
)

async function chooseFolder() {
  const selected = await open({ multiple: false, directory: true })
  if (selected && form.value) {
    form.value.output = { mode: 'folder', path: selected }
  }
}

async function save() {
  if (!form.value) return
  saving.value = true
  error.value = null
  saved.value = false
  try {
    await store.save(structuredClone(toRaw(form.value)))
    saved.value = true
  } catch (err) {
    error.value = errorMessage(t, err)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <div class="page">
    <header class="page__header">
      <h1 class="page__title">
        {{ t('settings.title') }}
      </h1>
    </header>

    <p v-if="!form && !error">
      {{ t('common.loading') }}
    </p>

    <form
      v-if="form"
      class="settings"
      @submit.prevent="save"
    >
      <section class="card field">
        <label
          class="field__label"
          for="language"
        >{{ t('settings.language.label') }}</label>
        <select
          id="language"
          v-model="form.language"
          class="input"
        >
          <option
            v-for="option in LANGUAGES"
            :key="option.value"
            :value="option.value"
          >
            {{ t(option.key) }}
          </option>
        </select>
      </section>

      <fieldset class="card field">
        <legend class="field__label">
          {{ t('settings.output.label') }}
        </legend>
        <label class="radio">
          <input
            v-model="outputMode"
            type="radio"
            value="sameFolder"
          >
          {{ t('settings.output.sameFolder') }}
        </label>
        <label class="radio">
          <input
            v-model="outputMode"
            type="radio"
            value="folder"
          >
          {{ t('settings.output.folder') }}
        </label>
        <div
          v-if="outputMode === 'folder'"
          class="field__row"
        >
          <code class="field__path">{{ outputFolder || t('settings.output.noFolder') }}</code>
          <button
            type="button"
            class="btn btn--small"
            @click="chooseFolder"
          >
            {{ t('settings.output.chooseFolder') }}
          </button>
        </div>

        <label
          class="field__label field__label--spaced"
          for="suffix"
        >{{ t('settings.suffix.label') }}</label>
        <input
          id="suffix"
          v-model.trim="form.outputSuffix"
          class="input"
          type="text"
          maxlength="40"
          required
        >
        <p class="field__help">
          {{ t('settings.suffix.help', { suffix: form.outputSuffix }) }}
        </p>
      </fieldset>

      <section class="card field">
        <label
          class="field__label"
          for="concurrency"
        >{{ t('settings.concurrency.label') }}</label>
        <input
          id="concurrency"
          v-model.number="form.concurrency"
          class="input input--number"
          type="number"
          min="1"
          :max="MAX_CONCURRENCY"
          required
        >
        <p class="field__help">
          {{ t('settings.concurrency.help') }}
        </p>

        <label
          class="field__label field__label--spaced"
          for="video-concurrency"
        >{{ t('settings.videoConcurrency.label') }}</label>
        <input
          id="video-concurrency"
          v-model.number="form.videoConcurrency"
          class="input input--number"
          type="number"
          min="1"
          :max="MAX_VIDEO_CONCURRENCY"
          required
        >
        <p class="field__help">
          {{ t('settings.videoConcurrency.help') }}
        </p>
      </section>

      <fieldset class="card field">
        <legend class="field__label">
          {{ t('settings.timeouts.label') }}
        </legend>
        <p class="field__help">
          {{ t('settings.timeouts.help') }}
        </p>
        <div class="timeouts">
          <label
            v-for="category in CATEGORIES"
            :key="category"
            class="timeouts__item"
          >
            <span>{{ t(`categories.${category}`) }}</span>
            <span class="timeouts__input">
              <input
                v-model.number="form.timeouts[category]"
                class="input input--number"
                type="number"
                min="1"
                :max="MAX_TIMEOUT_MINUTES"
                required
              >
              {{ t('common.minutes') }}
            </span>
          </label>
        </div>
      </fieldset>

      <div class="settings__actions">
        <button
          type="submit"
          class="btn btn--primary"
          :disabled="saving"
        >
          {{ t('settings.save') }}
        </button>
        <span
          v-if="saved"
          class="settings__saved"
          role="status"
        >{{ t('settings.saved') }}</span>
      </div>
    </form>

    <p
      v-if="error"
      class="alert alert--danger"
      role="alert"
    >
      {{ error }}
    </p>
  </div>
</template>

<style scoped>
.settings {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.field {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-1);
  margin: 0;
}

.field__label {
  padding: 0;
  font-weight: 600;
}

.field__label--spaced {
  margin-top: var(--space-3);
}

.field__help {
  margin: 0;
  color: var(--color-muted);
  font-size: 0.875rem;
}

.field__row {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--space-2);
}

.field__path {
  overflow-wrap: anywhere;
}

.radio {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.timeouts {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: var(--space-2) var(--space-4);
  width: 100%;
  margin-top: var(--space-2);
}

.timeouts__item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
}

.timeouts__input {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  color: var(--color-muted);
}

.settings__actions {
  display: flex;
  align-items: center;
  gap: var(--space-3);
}

.settings__saved {
  color: var(--color-success);
}
</style>

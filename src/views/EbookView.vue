<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import DropZone from '../components/DropZone.vue'
import FileList from '../components/FileList.vue'
import FormatChoiceOptions from '../components/options/FormatChoiceOptions.vue'
import { useOperationScreen } from '../composables/useOperationScreen'
import { EBOOK_FORMATS, buildEbookRequests, defaultEbookOptions, unavailableReason } from '../operations/ebook'
import type { Problem } from '../operations/image'
import type { ToolId } from '../types/ToolId'

const { t } = useI18n()
const options = ref(defaultEbookOptions())
const screen = useOperationScreen('ebook', 'ebook', [options])
const { files, filesStore, toolsStore } = screen

const hasTool = (id: ToolId) => toolsStore.isAvailable(id)
const translate = (problem: Problem | null) => (problem ? t(problem.key, problem.params ?? {}) : null)
const problem = computed(() => translate(unavailableReason(files.value, options.value, hasTool)))
const jobCount = computed(() => files.value.length)

function start() {
  if (problem.value) return
  void screen.start(buildEbookRequests(files.value, options.value))
}
</script>

<template>
  <div class="page">
    <header class="page__header">
      <h1 class="page__title">
        {{ t('ebook.title') }}
      </h1>
      <p class="page__subtitle">
        {{ t('ebook.subtitle') }}
      </p>
    </header>
    <DropZone @files="screen.addPaths" />
    <ul
      v-if="screen.addErrors.value.length > 0"
      class="alert alert--danger"
      role="alert"
    >
      <li
        v-for="error in screen.addErrors.value"
        :key="error"
      >
        {{ error }}
      </li>
    </ul>
    <p
      v-if="screen.addNotice.value"
      class="alert alert--info"
      role="status"
    >
      {{ screen.addNotice.value }}
    </p>
    <FileList
      v-if="files.length > 0"
      :files="files"
      @remove="filesStore.remove"
      @clear="filesStore.clear('ebook')"
    />
    <p
      v-if="screen.otherCount.value > 0"
      class="page__subtitle"
    >
      {{ t('ebook.otherFiles', screen.otherCount.value) }}
    </p>
    <template v-if="files.length > 0">
      <section
        class="card options"
        :aria-label="t('operations.options')"
      >
        <h2 class="section-title">
          {{ t('ebook.convertTitle') }}
        </h2>
        <FormatChoiceOptions
          v-model="options.format"
          name="ebook-format"
          prefix="ebook"
          :formats="EBOOK_FORMATS"
        />
      </section>
      <div class="actions">
        <button
          type="button"
          class="btn btn--primary"
          :disabled="!!problem || screen.starting.value"
          @click="start"
        >
          {{ t('ebook.start', jobCount) }}
        </button>
        <span
          v-if="problem"
          class="actions__problem"
          role="status"
        >{{ problem }}</span>
      </div>
      <p
        v-if="screen.started.value"
        class="alert alert--success"
        role="status"
      >
        {{ t('ebook.started', screen.started.value) }}
      </p>
      <p
        v-if="screen.startError.value"
        class="alert alert--danger"
        role="alert"
      >
        {{ screen.startError.value }}
      </p>
    </template>
  </div>
</template>

<style scoped>
.actions {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--space-3);
}

.actions__problem {
  color: var(--color-warning);
  font-size: 0.875rem;
}
</style>

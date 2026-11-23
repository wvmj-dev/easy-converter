<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import DropZone from '../components/DropZone.vue'
import FileList from '../components/FileList.vue'
import OperationPicker, { type OperationChoice } from '../components/OperationPicker.vue'
import FormatChoiceOptions from '../components/options/FormatChoiceOptions.vue'
import { useOperationScreen } from '../composables/useOperationScreen'
import {
  ARCHIVE_FORMATS,
  ARCHIVE_OPERATIONS,
  buildArchiveRequests,
  defaultArchiveOptions,
  unavailableReason,
  type ArchiveOperation,
} from '../operations/archive'
import type { Problem } from '../operations/image'
import type { ToolId } from '../types/ToolId'

const { t } = useI18n()
const operation = ref<ArchiveOperation>('extract')
const options = ref(defaultArchiveOptions())
const screen = useOperationScreen('archive', 'archive', [operation, options])
const { filesStore, toolsStore } = screen
// Any file can go into a new archive, so this screen shows the whole list.
const files = computed(() => filesStore.files)

const hasTool = (id: ToolId) => toolsStore.isAvailable(id)
const translate = (problem: Problem | null) => (problem ? t(problem.key, problem.params ?? {}) : null)

const choices = computed<OperationChoice[]>(() =>
  ARCHIVE_OPERATIONS.map((id) => ({
    id,
    label: t(`archive.ops.${id}.label`),
    description: t(`archive.ops.${id}.description`),
    unavailable: files.value.length > 0 ? translate(unavailableReason(id, files.value, hasTool)) : null,
  })),
)
const problem = computed(() => translate(unavailableReason(operation.value, files.value, hasTool)))
const jobCount = computed(() => (operation.value === 'create' ? 1 : files.value.length))

// The screen's notice counts archives only; here any new file counts.
async function addPaths(paths: string[]) {
  const before = filesStore.files.length
  await screen.addPaths(paths)
  if (filesStore.files.length > before) screen.addNotice.value = null
}

function start() {
  if (problem.value) return
  void screen.start(buildArchiveRequests(operation.value, files.value, options.value))
}
</script>

<template>
  <div class="page">
    <header class="page__header">
      <h1 class="page__title">
        {{ t('archive.title') }}
      </h1>
      <p class="page__subtitle">
        {{ t('archive.subtitle') }}
      </p>
    </header>
    <DropZone @files="addPaths" />
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
      @clear="filesStore.clear()"
    />
    <template v-if="files.length > 0">
      <OperationPicker
        v-model="operation"
        name="archive-operation"
        :operations="choices"
      />
      <section
        class="card options"
        :aria-label="t('operations.options')"
      >
        <h2 class="section-title">
          {{ t(`archive.ops.${operation}.label`) }}
        </h2>
        <FormatChoiceOptions
          v-if="operation === 'create'"
          v-model="options.create.format"
          name="archive-format"
          prefix="archive"
          :formats="ARCHIVE_FORMATS"
        />
        <p
          v-else
          class="field__help"
        >
          {{ t('archive.ops.extract.help') }}
        </p>
      </section>
      <div class="actions">
        <button
          type="button"
          class="btn btn--primary"
          :disabled="!!problem || screen.starting.value"
          @click="start"
        >
          {{ t('archive.start', jobCount) }}
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
        {{ t('archive.started', screen.started.value) }}
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

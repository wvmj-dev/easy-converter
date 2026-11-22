<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import DropZone from '../components/DropZone.vue'
import FileList from '../components/FileList.vue'
import OperationPicker, { type OperationChoice } from '../components/OperationPicker.vue'
import FormatChoiceOptions from '../components/options/FormatChoiceOptions.vue'
import PresentationToImagesOptions from '../components/options/PresentationToImagesOptions.vue'
import { useOperationScreen } from '../composables/useOperationScreen'
import type { Problem } from '../operations/image'
import {
  PRESENTATION_FORMATS,
  PRESENTATION_OPERATIONS,
  buildPresentationRequests,
  defaultPresentationOptions,
  unavailableReason,
  validateOptions,
  type PresentationOperation,
} from '../operations/presentation'
import type { ToolId } from '../types/ToolId'

const { t } = useI18n()
const operation = ref<PresentationOperation>('convert')
const options = ref(defaultPresentationOptions())
const screen = useOperationScreen('presentation', 'presentation', [operation, options])
const { files, filesStore, toolsStore } = screen

const hasTool = (id: ToolId) => toolsStore.isAvailable(id)
const translate = (problem: Problem | null) => (problem ? t(problem.key, problem.params ?? {}) : null)

const choices = computed<OperationChoice[]>(() =>
  PRESENTATION_OPERATIONS.map((id) => ({
    id,
    label: t(`presentation.ops.${id}.label`),
    description: t(`presentation.ops.${id}.description`),
    unavailable: files.value.length > 0 ? translate(unavailableReason(id, files.value, hasTool)) : null,
  })),
)
const problem = computed(
  () =>
    translate(unavailableReason(operation.value, files.value, hasTool)) ??
    translate(validateOptions(operation.value, options.value, files.value)),
)
const jobCount = computed(() => files.value.length)

function start() {
  if (problem.value) return
  void screen.start(buildPresentationRequests(operation.value, files.value, options.value))
}
</script>

<template>
  <div class="page">
    <header class="page__header">
      <h1 class="page__title">
        {{ t('presentation.title') }}
      </h1>
      <p class="page__subtitle">
        {{ t('presentation.subtitle') }}
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
      @clear="filesStore.clear('presentation')"
    />
    <p
      v-if="screen.otherCount.value > 0"
      class="page__subtitle"
    >
      {{ t('presentation.otherFiles', screen.otherCount.value) }}
    </p>
    <template v-if="files.length > 0">
      <OperationPicker
        v-model="operation"
        name="presentation-operation"
        :operations="choices"
      />
      <section
        class="card options"
        :aria-label="t('operations.options')"
      >
        <h2 class="section-title">
          {{ t(`presentation.ops.${operation}.label`) }}
        </h2>
        <FormatChoiceOptions
          v-if="operation === 'convert'"
          v-model="options.convert.format"
          name="presentation-format"
          prefix="presentation"
          :formats="PRESENTATION_FORMATS"
        />
        <PresentationToImagesOptions
          v-else
          v-model="options.toImages"
        />
      </section>
      <div class="actions">
        <button
          type="button"
          class="btn btn--primary"
          :disabled="!!problem || screen.starting.value"
          @click="start"
        >
          {{ t('presentation.start', jobCount) }}
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
        {{ t('presentation.started', screen.started.value) }}
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

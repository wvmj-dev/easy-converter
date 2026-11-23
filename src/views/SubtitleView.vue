<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import DropZone from '../components/DropZone.vue'
import FileList from '../components/FileList.vue'
import OperationPicker, { type OperationChoice } from '../components/OperationPicker.vue'
import FormatChoiceOptions from '../components/options/FormatChoiceOptions.vue'
import SubtitleShiftOptions from '../components/options/SubtitleShiftOptions.vue'
import { useOperationScreen } from '../composables/useOperationScreen'
import type { Problem } from '../operations/image'
import {
  SUBTITLE_FORMATS,
  SUBTITLE_OPERATIONS,
  buildSubtitleRequests,
  defaultSubtitleOptions,
  unavailableReason,
  validateOptions,
  type SubtitleOperation,
} from '../operations/subtitle'

const { t } = useI18n()
const operation = ref<SubtitleOperation>('convert')
const options = ref(defaultSubtitleOptions())
const screen = useOperationScreen('subtitle', 'subtitle', [operation, options])
const { files, filesStore } = screen

const translate = (problem: Problem | null) => (problem ? t(problem.key, problem.params ?? {}) : null)

const choices = computed<OperationChoice[]>(() =>
  SUBTITLE_OPERATIONS.map((id) => ({
    id,
    label: t(`subtitle.ops.${id}.label`),
    description: t(`subtitle.ops.${id}.description`),
    unavailable: files.value.length > 0 ? translate(unavailableReason(files.value)) : null,
  })),
)
const problem = computed(
  () =>
    translate(unavailableReason(files.value)) ??
    translate(validateOptions(operation.value, options.value, files.value)),
)
const jobCount = computed(() => files.value.length)

function start() {
  if (problem.value) return
  void screen.start(buildSubtitleRequests(operation.value, files.value, options.value))
}
</script>

<template>
  <div class="page">
    <header class="page__header">
      <h1 class="page__title">
        {{ t('subtitle.title') }}
      </h1>
      <p class="page__subtitle">
        {{ t('subtitle.subtitle') }}
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
      @clear="filesStore.clear('subtitle')"
    />
    <p
      v-if="screen.otherCount.value > 0"
      class="page__subtitle"
    >
      {{ t('subtitle.otherFiles', screen.otherCount.value) }}
    </p>
    <template v-if="files.length > 0">
      <OperationPicker
        v-model="operation"
        name="subtitle-operation"
        :operations="choices"
      />
      <section
        class="card options"
        :aria-label="t('operations.options')"
      >
        <h2 class="section-title">
          {{ t(`subtitle.ops.${operation}.label`) }}
        </h2>
        <FormatChoiceOptions
          v-if="operation === 'convert'"
          v-model="options.convert.format"
          name="subtitle-format"
          prefix="subtitle"
          :formats="SUBTITLE_FORMATS"
        />
        <SubtitleShiftOptions
          v-else
          v-model="options.shift"
        />
      </section>
      <div class="actions">
        <button
          type="button"
          class="btn btn--primary"
          :disabled="!!problem || screen.starting.value"
          @click="start"
        >
          {{ t('subtitle.start', jobCount) }}
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
        {{ t('subtitle.started', screen.started.value) }}
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

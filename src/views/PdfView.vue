<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import DropZone from '../components/DropZone.vue'
import FileList from '../components/FileList.vue'
import OperationPicker, { type OperationChoice } from '../components/OperationPicker.vue'
import FormatChoiceOptions from '../components/options/FormatChoiceOptions.vue'
import PdfCompressOptions from '../components/options/PdfCompressOptions.vue'
import PdfOcrOptions from '../components/options/PdfOcrOptions.vue'
import PdfProtectOptions from '../components/options/PdfProtectOptions.vue'
import PdfRotateOptions from '../components/options/PdfRotateOptions.vue'
import PdfSelectPagesOptions from '../components/options/PdfSelectPagesOptions.vue'
import PdfSplitOptions from '../components/options/PdfSplitOptions.vue'
import PdfToImagesOptions from '../components/options/PdfToImagesOptions.vue'
import PdfUnlockOptions from '../components/options/PdfUnlockOptions.vue'
import type { Problem } from '../operations/image'
import {
  PDF_OPERATIONS,
  buildPdfRequests,
  clearPasswords,
  defaultOcrLanguages,
  defaultPdfOptions,
  unavailableReason,
  usableOcrLanguages,
  validateOptions,
  type PdfOperation,
} from '../operations/pdf'
import { api } from '../services/api'
import { useFilesStore } from '../stores/files'
import { useJobsStore } from '../stores/jobs'
import { useToolsStore } from '../stores/tools'
import type { ToolId } from '../types/ToolId'
import { errorMessage } from '../utils/errors'
import { baseName } from '../utils/format'

const { t } = useI18n()
const filesStore = useFilesStore()
const jobsStore = useJobsStore()
const toolsStore = useToolsStore()

const operation = ref<PdfOperation>('merge')
const options = ref(defaultPdfOptions())
const addErrors = ref<string[]>([])
const addNotice = ref<string | null>(null)
const startError = ref<string | null>(null)
const started = ref<number | null>(null)
const starting = ref(false)

const files = computed(() => filesStore.ofCategory('pdf'))
const otherCount = computed(() => filesStore.files.length - files.value.length)
const hasTool = (id: ToolId) => toolsStore.isAvailable(id)
const ocrLanguages = computed(() => usableOcrLanguages(toolsStore.byId('tesseract')?.languages ?? []))

onMounted(async () => {
  if (!toolsStore.loaded) {
    try {
      await toolsStore.load()
    } catch {
      // Without the list every tool counts as missing; the Tools screen explains why.
    }
  }
})
// Pick the default OCR languages once the installed ones are known.
watch(
  ocrLanguages,
  (installed) => {
    if (options.value.ocr.languages.length === 0) {
      options.value.ocr.languages = defaultOcrLanguages(installed)
    }
  },
  { immediate: true },
)

const translate = (problem: Problem | null) => (problem ? t(problem.key, problem.params ?? {}) : null)

const choices = computed<OperationChoice[]>(() =>
  PDF_OPERATIONS.map((id) => ({
    id,
    label: t(`pdf.ops.${id}.label`),
    description: t(`pdf.ops.${id}.description`),
    unavailable: files.value.length > 0 ? translate(unavailableReason(id, files.value, hasTool)) : null,
  })),
)

const problem = computed(
  () =>
    translate(unavailableReason(operation.value, files.value, hasTool)) ??
    translate(validateOptions(operation.value, options.value, files.value, hasTool)),
)
const jobCount = computed(() => (operation.value === 'merge' ? 1 : files.value.length))

watch(
  [operation, options, files],
  () => {
    started.value = null
    startError.value = null
  },
  { deep: true },
)

async function addPaths(paths: string[]) {
  addErrors.value = []
  addNotice.value = null
  const before = files.value.length
  const failures = await filesStore.add(paths)
  addErrors.value = failures.map(
    (f) => `${t('files.addFailed', { name: baseName(f.path) })}: ${errorMessage(t, f.error)}`,
  )
  if (failures.length < paths.length && files.value.length === before) {
    addNotice.value = t('pdf.noNewFiles')
  }
}

async function start() {
  if (problem.value || starting.value) return
  starting.value = true
  startError.value = null
  started.value = null
  await jobsStore.listen()
  const requests = buildPdfRequests(operation.value, files.value, options.value)
  const results = await Promise.allSettled(requests.map((request) => api.enqueueJob(request)))
  // The passwords were handed to the backend; do not keep them on screen.
  clearPasswords(options.value)
  const failed = results.filter((r): r is PromiseRejectedResult => r.status === 'rejected')
  const done = results.length - failed.length
  started.value = done > 0 ? done : null
  if (failed[0]) {
    startError.value = `${t('pdf.startFailed', failed.length)}: ${errorMessage(t, failed[0].reason)}`
  }
  starting.value = false
}
</script>

<template>
  <div class="page">
    <header class="page__header">
      <h1 class="page__title">
        {{ t('pdf.title') }}
      </h1>
      <p class="page__subtitle">
        {{ t('pdf.subtitle') }}
      </p>
    </header>
    <DropZone @files="addPaths" />
    <ul
      v-if="addErrors.length > 0"
      class="alert alert--danger"
      role="alert"
    >
      <li
        v-for="error in addErrors"
        :key="error"
      >
        {{ error }}
      </li>
    </ul>
    <p
      v-if="addNotice"
      class="alert alert--info"
      role="status"
    >
      {{ addNotice }}
    </p>
    <FileList
      v-if="files.length > 0"
      :files="files"
      :reorderable="operation === 'merge'"
      @remove="filesStore.remove"
      @clear="filesStore.clear('pdf')"
      @move="filesStore.move"
    />
    <p
      v-if="otherCount > 0"
      class="page__subtitle"
    >
      {{ t('pdf.otherFiles', otherCount) }}
    </p>
    <template v-if="files.length > 0">
      <OperationPicker
        v-model="operation"
        name="pdf-operation"
        :operations="choices"
      />
      <section
        class="card options"
        :aria-label="t('operations.options')"
      >
        <h2 class="section-title">
          {{ t(`pdf.ops.${operation}.label`) }}
        </h2>
        <PdfSplitOptions
          v-if="operation === 'split'"
          v-model="options.split"
        />
        <PdfSelectPagesOptions
          v-else-if="operation === 'selectPages'"
          v-model="options.selectPages"
        />
        <PdfRotateOptions
          v-else-if="operation === 'rotate'"
          v-model="options.rotate"
        />
        <PdfCompressOptions
          v-else-if="operation === 'compress'"
          v-model="options.compress"
          :strong-available="hasTool('ghostscript')"
        />
        <PdfToImagesOptions
          v-else-if="operation === 'toImages'"
          v-model="options.toImages"
        />
        <FormatChoiceOptions
          v-else-if="operation === 'toDocument'"
          v-model="options.toDocument.format"
          name="pdf-document-format"
          prefix="pdf.toDocument"
          :formats="['docx', 'odt']"
        />
        <PdfOcrOptions
          v-else-if="operation === 'ocr'"
          v-model="options.ocr"
          :languages="ocrLanguages"
        />
        <PdfProtectOptions
          v-else-if="operation === 'protect'"
          v-model="options.protect"
        />
        <PdfUnlockOptions
          v-else-if="operation === 'unlock'"
          v-model="options.unlock"
        />
        <p
          v-else
          class="field__help"
        >
          {{ t(`pdf.ops.${operation}.help`) }}
        </p>
      </section>
      <div class="actions">
        <button
          type="button"
          class="btn btn--primary"
          :disabled="!!problem || starting"
          @click="start"
        >
          {{ t('pdf.start', jobCount) }}
        </button>
        <span
          v-if="problem"
          class="actions__problem"
          role="status"
        >{{ problem }}</span>
      </div>
      <p
        v-if="started"
        class="alert alert--success"
        role="status"
      >
        {{ t('pdf.started', started) }}
      </p>
      <p
        v-if="startError"
        class="alert alert--danger"
        role="alert"
      >
        {{ startError }}
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

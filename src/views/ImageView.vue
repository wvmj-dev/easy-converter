<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import DropZone from '../components/DropZone.vue'
import FileList from '../components/FileList.vue'
import OperationPicker, { type OperationChoice } from '../components/OperationPicker.vue'
import ImageCompressOptions from '../components/options/ImageCompressOptions.vue'
import ImageConvertOptions from '../components/options/ImageConvertOptions.vue'
import ImageCropOptions from '../components/options/ImageCropOptions.vue'
import ImageFlipOptions from '../components/options/ImageFlipOptions.vue'
import ImageMergeOptions from '../components/options/ImageMergeOptions.vue'
import ImageResizeOptions from '../components/options/ImageResizeOptions.vue'
import ImageRotateOptions from '../components/options/ImageRotateOptions.vue'
import ImageToPdfOptions from '../components/options/ImageToPdfOptions.vue'
import {
  COMBINING_OPERATIONS,
  IMAGE_OPERATIONS,
  buildImageRequests,
  defaultImageOptions,
  fullArea,
  unavailableReason,
  validateOptions,
  type ImageOperation,
  type Problem,
} from '../operations/image'
import { api } from '../services/api'
import { useFilesStore } from '../stores/files'
import { useJobsStore } from '../stores/jobs'
import { errorMessage } from '../utils/errors'
import { baseName } from '../utils/format'

const { t } = useI18n()
const filesStore = useFilesStore()
const jobsStore = useJobsStore()

const operation = ref<ImageOperation>('convert')
const options = ref(defaultImageOptions())
const addErrors = ref<string[]>([])
const addNotice = ref<string | null>(null)
const startError = ref<string | null>(null)
const started = ref<number | null>(null)
const starting = ref(false)

const files = computed(() => filesStore.ofCategory('image'))
const otherCount = computed(() => filesStore.files.length - files.value.length)

const translate = (problem: Problem | null) => (problem ? t(problem.key, problem.params ?? {}) : null)

const choices = computed<OperationChoice[]>(() =>
  IMAGE_OPERATIONS.map((id) => ({
    id,
    label: t(`image.ops.${id}.label`),
    description: t(`image.ops.${id}.description`),
    unavailable: files.value.length > 0 ? translate(unavailableReason(id, files.value)) : null,
  })),
)

const problem = computed(
  () =>
    translate(unavailableReason(operation.value, files.value)) ??
    translate(validateOptions(operation.value, options.value, files.value)),
)
const combining = computed(() => COMBINING_OPERATIONS.includes(operation.value))
const jobCount = computed(() => (combining.value ? 1 : files.value.length))

// Start the crop area from the whole first image.
const cropReference = computed(() => files.value[0]?.image ?? null)
function resetCrop() {
  const area = fullArea(files.value[0])
  if (area) options.value.crop = area
}
watch(
  () => files.value[0]?.path,
  () => resetCrop(),
  { immediate: true },
)

// Any change makes the previous feedback stale.
watch([operation, options, files], () => {
  started.value = null
  startError.value = null
}, { deep: true })

async function addPaths(paths: string[]) {
  addErrors.value = []
  addNotice.value = null
  const before = files.value.length
  const failures = await filesStore.add(paths)
  addErrors.value = failures.map(
    (f) => `${t('files.addFailed', { name: baseName(f.path) })}: ${errorMessage(t, f.error)}`,
  )
  if (failures.length < paths.length && files.value.length === before) {
    addNotice.value = t('image.noNewImages')
  }
}

async function start() {
  if (problem.value || starting.value) return
  starting.value = true
  startError.value = null
  started.value = null
  // Listen before enqueueing so no event of the new jobs is missed.
  await jobsStore.listen()
  const requests = buildImageRequests(operation.value, files.value, options.value)
  const results = await Promise.allSettled(requests.map((request) => api.enqueueJob(request)))
  const failed = results.filter((r): r is PromiseRejectedResult => r.status === 'rejected')
  const done = results.length - failed.length
  started.value = done > 0 ? done : null
  if (failed[0]) {
    startError.value = `${t('image.startFailed', failed.length)}: ${errorMessage(t, failed[0].reason)}`
  }
  starting.value = false
}
</script>

<template>
  <div class="page">
    <header class="page__header">
      <h1 class="page__title">
        {{ t('image.title') }}
      </h1>
      <p class="page__subtitle">
        {{ t('image.subtitle') }}
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
      :reorderable="combining"
      @remove="filesStore.remove"
      @clear="filesStore.clear('image')"
      @move="filesStore.move"
    />
    <p
      v-if="otherCount > 0"
      class="page__subtitle"
    >
      {{ t('image.otherFiles', otherCount) }}
    </p>

    <template v-if="files.length > 0">
      <OperationPicker
        v-model="operation"
        name="image-operation"
        :operations="choices"
      />

      <section
        class="card options"
        :aria-label="t('operations.options')"
      >
        <h2 class="section-title">
          {{ t(`image.ops.${operation}.label`) }}
        </h2>
        <ImageConvertOptions
          v-if="operation === 'convert'"
          v-model="options.convert"
        />
        <ImageResizeOptions
          v-else-if="operation === 'resize'"
          v-model="options.resize"
        />
        <ImageCropOptions
          v-else-if="operation === 'crop'"
          v-model="options.crop"
          :reference="cropReference"
          :multiple="files.length > 1"
          @reset="resetCrop"
        />
        <ImageRotateOptions
          v-else-if="operation === 'rotate'"
          v-model="options.rotate"
        />
        <ImageFlipOptions
          v-else-if="operation === 'flip'"
          v-model="options.flip"
        />
        <ImageCompressOptions
          v-else-if="operation === 'compress'"
          v-model="options.compress"
        />
        <ImageMergeOptions
          v-else-if="operation === 'merge'"
          v-model="options.merge"
        />
        <ImageToPdfOptions
          v-else-if="operation === 'toPdf'"
          v-model="options.toPdf"
        />
        <p
          v-else
          class="field__help"
        >
          {{ t('image.strip.help') }}
        </p>
      </section>

      <div class="actions">
        <button
          type="button"
          class="btn btn--primary"
          :disabled="!!problem || starting"
          @click="start"
        >
          {{ t('image.start', jobCount) }}
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
        {{ t('image.started', started) }}
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

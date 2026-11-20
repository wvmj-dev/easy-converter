// State shared by the category screens: the files of one category, adding files and
// starting jobs, with the messages shown to the user.
import { computed, onMounted, ref, watch, type WatchSource } from 'vue'
import { useI18n } from 'vue-i18n'
import { api } from '../services/api'
import { useFilesStore } from '../stores/files'
import { useJobsStore } from '../stores/jobs'
import { useToolsStore } from '../stores/tools'
import type { JobCategory } from '../types/JobCategory'
import type { JobRequest } from '../types/JobRequest'
import { errorMessage } from '../utils/errors'
import { baseName } from '../utils/format'

/**
 * `prefix` is the i18n section of the screen, which must have `noNewFiles` and
 * `startFailed`. Changes to `watched` clear the result of the last start.
 */
export function useOperationScreen(category: JobCategory, prefix: string, watched: WatchSource[]) {
  const { t } = useI18n()
  const filesStore = useFilesStore()
  const jobsStore = useJobsStore()
  const toolsStore = useToolsStore()

  const addErrors = ref<string[]>([])
  const addNotice = ref<string | null>(null)
  const startError = ref<string | null>(null)
  const started = ref<number | null>(null)
  const starting = ref(false)

  const files = computed(() => filesStore.ofCategory(category))
  const otherCount = computed(() => filesStore.files.length - files.value.length)

  onMounted(async () => {
    if (!toolsStore.loaded) {
      try {
        await toolsStore.load()
      } catch {
        // Without the list every tool counts as missing; the Tools screen explains why.
      }
    }
  })

  watch(
    [...watched, files],
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
      addNotice.value = t(`${prefix}.noNewFiles`)
    }
  }

  async function start(requests: JobRequest[]) {
    if (starting.value) return
    starting.value = true
    startError.value = null
    started.value = null
    await jobsStore.listen()
    const results = await Promise.allSettled(requests.map((request) => api.enqueueJob(request)))
    const failed = results.filter((r): r is PromiseRejectedResult => r.status === 'rejected')
    const done = results.length - failed.length
    started.value = done > 0 ? done : null
    if (failed[0]) {
      startError.value = `${t(`${prefix}.startFailed`, failed.length)}: ${errorMessage(t, failed[0].reason)}`
    }
    starting.value = false
  }

  return {
    files,
    otherCount,
    addErrors,
    addNotice,
    startError,
    started,
    starting,
    addPaths,
    start,
    filesStore,
    toolsStore,
  }
}

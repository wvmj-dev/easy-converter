import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { api } from '../services/api'
import type { FileInfo } from '../types/FileInfo'
import type { JobCategory } from '../types/JobCategory'

export interface AddFailure {
  path: string
  error: unknown
}

/**
 * Files chosen by the user, shared by the home screen and the category screens.
 * Only paths and the metadata returned by `probe_file` are kept; never file contents.
 */
export const useFilesStore = defineStore('files', () => {
  const files = ref<FileInfo[]>([])

  const categories = computed(() => {
    const counts = new Map<JobCategory, number>()
    for (const file of files.value) {
      if (file.category) counts.set(file.category, (counts.get(file.category) ?? 0) + 1)
    }
    return counts
  })

  function ofCategory(category: JobCategory): FileInfo[] {
    return files.value.filter((f) => f.category === category)
  }

  /** Probes the paths and adds new ones in order. Returns the paths that failed. */
  async function add(paths: string[]): Promise<AddFailure[]> {
    const unique = [...new Set(paths)]
    const results = await Promise.allSettled(unique.map((path) => api.probeFile(path)))
    const failures: AddFailure[] = []
    results.forEach((result, index) => {
      if (result.status === 'fulfilled') {
        if (!files.value.some((f) => f.path === result.value.path)) {
          files.value.push(result.value)
        }
      } else {
        failures.push({ path: unique[index] ?? '', error: result.reason })
      }
    })
    return failures
  }

  function remove(path: string) {
    files.value = files.value.filter((f) => f.path !== path)
  }

  /** Removes the files of one category, or every file. */
  function clear(category?: JobCategory) {
    files.value = category ? files.value.filter((f) => f.category !== category) : []
  }

  /** Moves a file one position up (-1) or down (+1) among the files of its category. */
  function move(path: string, delta: -1 | 1) {
    const index = files.value.findIndex((f) => f.path === path)
    const file = files.value[index]
    if (!file) return
    let target = index + delta
    while (target >= 0 && target < files.value.length && files.value[target]?.category !== file.category) {
      target += delta
    }
    const other = files.value[target]
    if (!other) return
    const next = [...files.value]
    next[index] = other
    next[target] = file
    files.value = next
  }

  return { files, categories, ofCategory, add, remove, clear, move }
})

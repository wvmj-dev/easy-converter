import { ref } from 'vue'
import { defineStore } from 'pinia'
import { api } from '../services/api'
import type { ToolId } from '../types/ToolId'
import type { ToolStatus } from '../types/ToolStatus'

export const useToolsStore = defineStore('tools', () => {
  const statuses = ref<ToolStatus[]>([])
  const loading = ref(false)
  const loaded = ref(false)

  async function run(fetch: () => Promise<ToolStatus[]>) {
    loading.value = true
    try {
      statuses.value = await fetch()
      loaded.value = true
    } finally {
      loading.value = false
    }
  }

  const load = () => run(api.getToolsStatus)
  const refresh = () => run(api.refreshTools)

  function byId(id: ToolId): ToolStatus | undefined {
    return statuses.value.find((s) => s.id === id)
  }

  function isAvailable(id: ToolId): boolean {
    return byId(id)?.available ?? false
  }

  return { statuses, loading, loaded, load, refresh, byId, isAvailable }
})

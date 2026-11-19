import { onMounted } from 'vue'
import { storeToRefs } from 'pinia'
import { useToolsStore } from '../stores/tools'

/** Tool statuses, loaded on first use. */
export function useTools() {
  const store = useToolsStore()
  onMounted(() => {
    if (!store.loaded && !store.loading) {
      void store.load()
    }
  })
  const { statuses, loading } = storeToRefs(store)
  return { statuses, loading, refresh: store.refresh, isAvailable: store.isAvailable }
}

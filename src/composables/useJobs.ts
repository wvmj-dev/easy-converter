import { onMounted } from 'vue'
import { storeToRefs } from 'pinia'
import { useJobsStore } from '../stores/jobs'

/** Job list kept up to date by backend events. */
export function useJobs() {
  const store = useJobsStore()
  onMounted(() => {
    void store.listen()
  })
  const { jobs, active, hasFinished } = storeToRefs(store)
  return { jobs, active, hasFinished, cancel: store.cancel, clearFinished: store.clearFinished }
}

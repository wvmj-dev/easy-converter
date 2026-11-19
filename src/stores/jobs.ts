import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { api, onJobEvent } from '../services/api'
import type { Job } from '../types/Job'
import type { JobStatus } from '../types/JobStatus'

const FINAL: JobStatus[] = ['done', 'failed', 'cancelled']

export function isFinal(status: JobStatus): boolean {
  return FINAL.includes(status)
}

export const useJobsStore = defineStore('jobs', () => {
  const jobs = ref<Job[]>([])
  let unlisten: UnlistenFn | null = null
  let starting: Promise<void> | null = null

  const active = computed(() => jobs.value.filter((job) => !isFinal(job.status)))
  const hasFinished = computed(() => jobs.value.some((job) => isFinal(job.status)))

  /** Applies a job snapshot from an event. A finished job never goes back to running. */
  function upsert(job: Job) {
    const index = jobs.value.findIndex((j) => j.id === job.id)
    if (index === -1) {
      jobs.value.push(job)
      return
    }
    const current = jobs.value[index]
    if (current && isFinal(current.status) && !isFinal(job.status)) {
      return
    }
    jobs.value[index] = job
  }

  function listen(): Promise<void> {
    starting ??= onJobEvent(upsert).then((fn) => {
      unlisten = fn
    })
    return starting
  }

  function stopListening() {
    unlisten?.()
    unlisten = null
    starting = null
  }

  async function cancel(id: number) {
    await api.cancelJob(id)
  }

  function clearFinished() {
    jobs.value = jobs.value.filter((job) => !isFinal(job.status))
  }

  return { jobs, active, hasFinished, upsert, listen, stopListening, cancel, clearFinished }
})

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useJobsStore } from './jobs'
import type { Job } from '../types/Job'

const handlers = new Map<string, (event: { payload: Job }) => void>()
const invoke = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))
vi.mock('@tauri-apps/api/event', () => ({
  listen: async (name: string, handler: (event: { payload: Job }) => void) => {
    handlers.set(name, handler)
    return () => handlers.delete(name)
  },
}))

function job(overrides: Partial<Job>): Job {
  return {
    id: 1,
    kind: 'test',
    category: 'image',
    inputs: ['/a.png'],
    outputs: [],
    status: 'queued',
    progress: null,
    error: null,
    ...overrides,
  }
}

function emit(name: string, payload: Job) {
  handlers.get(name)?.({ payload })
}

describe('jobs store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    handlers.clear()
    invoke.mockReset()
  })

  it('follows a job through its events', async () => {
    const store = useJobsStore()
    await store.listen()

    emit('job:created', job({}))
    emit('job:progress', job({ status: 'running', progress: 40 }))
    expect(store.jobs).toHaveLength(1)
    expect(store.active).toHaveLength(1)
    expect(store.jobs[0]?.progress).toBe(40)

    emit('job:finished', job({ status: 'done', progress: 100, outputs: ['/a-convertido.png'] }))
    expect(store.active).toHaveLength(0)
    expect(store.hasFinished).toBe(true)
  })

  it('ignores progress that arrives after the job finished', async () => {
    const store = useJobsStore()
    await store.listen()

    emit('job:failed', job({ status: 'failed', error: { code: 'timeout', message: 'timed out' } }))
    emit('job:progress', job({ status: 'running', progress: 90 }))

    expect(store.jobs[0]?.status).toBe('failed')
  })

  it('subscribes only once and clears finished jobs', async () => {
    const store = useJobsStore()
    await Promise.all([store.listen(), store.listen()])
    expect(handlers.size).toBe(4)

    emit('job:created', job({ id: 1 }))
    emit('job:finished', job({ id: 2, status: 'cancelled' }))
    store.clearFinished()

    expect(store.jobs.map((j) => j.id)).toEqual([1])
  })

  it('cancels through the backend', async () => {
    const store = useJobsStore()
    await store.cancel(7)
    expect(invoke).toHaveBeenCalledWith('cancel_job', { id: 7 })
  })
})

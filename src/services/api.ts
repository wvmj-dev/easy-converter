// Typed wrappers around the backend commands. The frontend never reads file contents:
// it only passes paths and parameters, which the backend validates.
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { ErrorPayload } from '../types/ErrorPayload'
import type { FileInfo } from '../types/FileInfo'
import type { Job } from '../types/Job'
import type { JobRequest } from '../types/JobRequest'
import type { Settings } from '../types/Settings'
import type { ToolStatus } from '../types/ToolStatus'

export const JOB_EVENTS = ['job:created', 'job:progress', 'job:finished', 'job:failed'] as const

export const api = {
  enqueueJob: (request: JobRequest) => invoke<number>('enqueue_job', { request }),
  cancelJob: (id: number) => invoke<void>('cancel_job', { id }),
  getToolsStatus: () => invoke<ToolStatus[]>('get_tools_status'),
  refreshTools: () => invoke<ToolStatus[]>('refresh_tools'),
  probeFile: (path: string) => invoke<FileInfo>('probe_file', { path }),
  getSettings: () => invoke<Settings>('get_settings'),
  updateSettings: (newSettings: Settings) => invoke<Settings>('update_settings', { newSettings }),
  openOutput: (path: string) => invoke<void>('open_output', { path }),
  revealOutput: (path: string) => invoke<void>('reveal_output', { path }),
}

/** Subscribes to every job event. Each one carries a full job snapshot. */
export async function onJobEvent(handler: (job: Job) => void): Promise<UnlistenFn> {
  const unlisteners = await Promise.all(
    JOB_EVENTS.map((name) => listen<Job>(name, (event) => handler(event.payload))),
  )
  return () => unlisteners.forEach((unlisten) => unlisten())
}

export function isErrorPayload(value: unknown): value is ErrorPayload {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as ErrorPayload).code === 'string' &&
    typeof (value as ErrorPayload).message === 'string'
  )
}

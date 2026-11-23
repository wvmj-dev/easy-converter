import type { Composer } from 'vue-i18n'
import { isErrorPayload } from '../services/api'

/** A translated error message and, apart, the technical detail sent by the backend. */
export interface ErrorParts {
  message: string
  detail: string | null
}

/** Codes whose detail adds nothing the message does not already say. */
const NO_DETAIL = new Set(['timeout', 'cancelled', 'toolMissing'])

export function errorParts(t: Composer['t'], error: unknown): ErrorParts {
  if (isErrorPayload(error)) {
    const detail = error.detail ?? null
    if (error.code === 'toolMissing' && detail) {
      return { message: t('errors.toolMissingNamed', { tool: detail }), detail: null }
    }
    return { message: t(`errors.${error.code}`), detail: NO_DETAIL.has(error.code) ? null : detail }
  }
  return { message: t('errors.unknown'), detail: error instanceof Error ? error.message : null }
}

/** Translated message for an error returned by the backend, with its technical detail. */
export function errorMessage(t: Composer['t'], error: unknown): string {
  const { message, detail } = errorParts(t, error)
  return detail ? `${message} (${detail})` : message
}

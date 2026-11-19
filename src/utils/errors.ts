import type { Composer } from 'vue-i18n'
import { isErrorPayload } from '../services/api'

/** Translated message for an error returned by the backend, with its technical detail. */
export function errorMessage(t: Composer['t'], error: unknown): string {
  if (isErrorPayload(error)) {
    return `${t(`errors.${error.code}`)} (${error.message})`
  }
  return error instanceof Error ? `${t('errors.unknown')} (${error.message})` : t('errors.unknown')
}

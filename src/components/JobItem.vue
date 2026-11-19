<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { api } from '../services/api'
import { isFinal } from '../stores/jobs'
import type { Job } from '../types/Job'
import { errorMessage } from '../utils/errors'
import { baseName } from '../utils/format'

const props = defineProps<{ job: Job }>()
const emit = defineEmits<{ cancel: [id: number] }>()
const { t } = useI18n()
const actionError = ref<string | null>(null)

const title = computed(() => {
  const first = props.job.inputs[0]
  const name = first ? baseName(first) : props.job.kind
  return props.job.inputs.length > 1 ? `${name} +${props.job.inputs.length - 1}` : name
})
const firstOutput = computed(() => props.job.outputs[0] ?? null)
const percent = computed(() =>
  props.job.progress === null ? null : Math.round(props.job.progress),
)

async function run(action: (path: string) => Promise<void>) {
  if (!firstOutput.value) return
  actionError.value = null
  try {
    await action(firstOutput.value)
  } catch (error) {
    actionError.value = errorMessage(t, error)
  }
}
</script>

<template>
  <li
    class="job"
    :class="`job--${job.status}`"
  >
    <div class="job__row">
      <span
        class="job__title"
        :title="job.inputs.join('\n')"
      >{{ title }}</span>
      <span class="job__status">{{ t(`jobs.status.${job.status}`) }}</span>
    </div>

    <div
      v-if="job.status === 'running'"
      class="progress"
      :class="{ 'progress--indeterminate': percent === null }"
      role="progressbar"
      :aria-label="t('jobs.progress')"
      :aria-valuenow="percent ?? undefined"
      aria-valuemin="0"
      aria-valuemax="100"
    >
      <div
        class="progress__bar"
        :style="percent === null ? undefined : { width: `${percent}%` }"
      />
    </div>

    <p
      v-if="job.status === 'failed' && job.error"
      class="job__error"
    >
      {{ errorMessage(t, job.error) }}
    </p>
    <p
      v-if="actionError"
      class="job__error"
    >
      {{ actionError }}
    </p>

    <div class="job__actions">
      <button
        v-if="!isFinal(job.status)"
        type="button"
        class="btn btn--ghost btn--small"
        @click="emit('cancel', job.id)"
      >
        {{ t('jobs.cancel') }}
      </button>
      <template v-if="job.status === 'done' && firstOutput">
        <button
          type="button"
          class="btn btn--ghost btn--small"
          @click="run(api.openOutput)"
        >
          {{ t('jobs.openFile') }}
        </button>
        <button
          type="button"
          class="btn btn--ghost btn--small"
          @click="run(api.revealOutput)"
        >
          {{ t('jobs.openFolder') }}
        </button>
      </template>
    </div>
  </li>
</template>

<style scoped>
.job {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  padding: var(--space-2) 0;
  border-top: 1px solid var(--color-border);
}

.job__row {
  display: flex;
  justify-content: space-between;
  gap: var(--space-2);
}

.job__title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.job__status {
  flex-shrink: 0;
  color: var(--color-muted);
  font-size: 0.875rem;
}

.job--failed .job__status {
  color: var(--color-danger);
}

.job--done .job__status {
  color: var(--color-success);
}

.job__error {
  margin: 0;
  color: var(--color-danger);
  font-size: 0.875rem;
  overflow-wrap: anywhere;
}

.job__actions {
  display: flex;
  gap: var(--space-1);
}

.job__actions:empty {
  display: none;
}

.progress {
  position: relative;
  height: 6px;
  overflow: hidden;
  border-radius: 3px;
  background: var(--color-border);
}

.progress__bar {
  height: 100%;
  background: var(--color-accent);
  transition: width 0.2s;
}

.progress--indeterminate .progress__bar {
  position: absolute;
  width: 30%;
  animation: indeterminate 1.2s ease-in-out infinite;
}

@keyframes indeterminate {
  from { left: -30%; }
  to { left: 100%; }
}

@media (prefers-reduced-motion: reduce) {
  .progress--indeterminate .progress__bar {
    left: 0;
    width: 100%;
    opacity: 0.5;
    animation: none;
  }
}
</style>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { useJobs } from '../composables/useJobs'
import JobItem from './JobItem.vue'

const { t } = useI18n()
const { jobs, hasFinished, cancel, clearFinished } = useJobs()
</script>

<template>
  <aside
    v-if="jobs.length > 0"
    class="job-panel"
    :aria-label="t('jobs.title')"
  >
    <header class="job-panel__header">
      <h2 class="section-title">
        {{ t('jobs.title') }}
      </h2>
      <button
        v-if="hasFinished"
        type="button"
        class="btn btn--ghost btn--small"
        @click="clearFinished"
      >
        {{ t('jobs.clearFinished') }}
      </button>
    </header>
    <ul class="job-panel__list">
      <JobItem
        v-for="job in jobs"
        :key="job.id"
        :job="job"
        @cancel="cancel"
      />
    </ul>
  </aside>
</template>

<style scoped>
.job-panel {
  display: flex;
  flex-direction: column;
  width: 320px;
  flex-shrink: 0;
  padding: var(--space-4);
  overflow-y: auto;
  border-left: 1px solid var(--color-border);
  background: var(--color-surface);
}

.job-panel__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
}

.job-panel__list {
  margin: var(--space-2) 0 0;
  padding: 0;
  list-style: none;
}
</style>

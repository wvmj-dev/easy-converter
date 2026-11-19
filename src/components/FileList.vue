<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import type { FileInfo } from '../types/FileInfo'
import { formatBytes } from '../utils/format'

defineProps<{ files: FileInfo[] }>()
const emit = defineEmits<{ remove: [path: string]; clear: [] }>()
const { t, locale } = useI18n()
</script>

<template>
  <section class="file-list card">
    <header class="file-list__header">
      <h2 class="section-title">
        {{ t('files.title', { count: files.length }) }}
      </h2>
      <button
        type="button"
        class="btn btn--ghost"
        @click="emit('clear')"
      >
        {{ t('files.clear') }}
      </button>
    </header>
    <ul class="file-list__items">
      <li
        v-for="file in files"
        :key="file.path"
        class="file-list__item"
      >
        <div class="file-list__info">
          <span
            class="file-list__name"
            :title="file.path"
          >{{ file.name }}</span>
          <span class="file-list__meta">
            <span
              class="badge"
              :class="{ 'badge--warning': !file.category }"
            >
              {{ file.category ? t(`categories.${file.category}`) : t('files.unsupported') }}
            </span>
            {{ formatBytes(file.size, locale) }}
          </span>
        </div>
        <button
          type="button"
          class="btn btn--ghost btn--small"
          @click="emit('remove', file.path)"
        >
          {{ t('files.remove') }}
        </button>
      </li>
    </ul>
  </section>
</template>

<style scoped>
.file-list__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
}

.file-list__items {
  margin: var(--space-2) 0 0;
  padding: 0;
  list-style: none;
}

.file-list__item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  padding: var(--space-2) 0;
  border-top: 1px solid var(--color-border);
}

.file-list__info {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.file-list__name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.file-list__meta {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  color: var(--color-muted);
  font-size: 0.875rem;
}
</style>

<script setup lang="ts">
import { open } from '@tauri-apps/plugin-dialog'
import { useI18n } from 'vue-i18n'
import { useDragDrop } from '../composables/useDragDrop'

const emit = defineEmits<{ files: [paths: string[]] }>()
const { t } = useI18n()

const { isOver } = useDragDrop((paths) => emit('files', paths))

async function browse() {
  const selected = await open({ multiple: true, directory: false })
  if (selected && selected.length > 0) {
    emit('files', selected)
  }
}
</script>

<template>
  <section
    class="drop-zone"
    :class="{ 'drop-zone--over': isOver }"
  >
    <p class="drop-zone__title">
      {{ isOver ? t('dropZone.release') : t('dropZone.title') }}
    </p>
    <p class="drop-zone__or">
      {{ t('dropZone.or') }}
    </p>
    <button
      type="button"
      class="btn btn--primary"
      @click="browse"
    >
      {{ t('dropZone.browse') }}
    </button>
  </section>
</template>

<style scoped>
.drop-zone {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-6) var(--space-4);
  border: 2px dashed var(--color-border-strong);
  border-radius: var(--radius-lg);
  background: var(--color-surface);
  text-align: center;
  transition: border-color 0.15s, background-color 0.15s;
}

.drop-zone--over {
  border-color: var(--color-accent);
  background: var(--color-accent-soft);
}

.drop-zone__title {
  margin: 0;
  font-size: 1.125rem;
  font-weight: 600;
}

.drop-zone__or {
  margin: 0;
  color: var(--color-muted);
}
</style>

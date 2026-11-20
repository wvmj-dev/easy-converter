<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { RouterLink } from 'vue-router'
import DropZone from '../components/DropZone.vue'
import FileList from '../components/FileList.vue'
import { useFilesStore } from '../stores/files'
import type { JobCategory } from '../types/JobCategory'
import { errorMessage } from '../utils/errors'
import { baseName } from '../utils/format'

/** Categories that already have a screen with operations. */
const SCREENS: Partial<Record<JobCategory, string>> = {
  image: '/image',
  pdf: '/pdf',
  video: '/video',
  audio: '/audio',
}

const { t } = useI18n()
const store = useFilesStore()
const errors = ref<string[]>([])

const ready = computed(() =>
  [...store.categories]
    .filter(([category]) => SCREENS[category])
    .map(([category, count]) => ({ category, count, to: SCREENS[category] ?? '/' })),
)
const pending = computed(() =>
  [...store.categories.keys()].filter((category) => !SCREENS[category]),
)

async function addPaths(paths: string[]) {
  const failures = await store.add(paths)
  errors.value = failures.map(
    (f) => `${t('files.addFailed', { name: baseName(f.path) })}: ${errorMessage(t, f.error)}`,
  )
}
</script>

<template>
  <div class="page">
    <header class="page__header">
      <h1 class="page__title">
        {{ t('home.title') }}
      </h1>
      <p class="page__subtitle">
        {{ t('home.subtitle') }}
      </p>
    </header>

    <DropZone @files="addPaths" />

    <ul
      v-if="errors.length > 0"
      class="alert alert--danger"
      role="alert"
    >
      <li
        v-for="error in errors"
        :key="error"
      >
        {{ error }}
      </li>
    </ul>

    <template v-if="store.files.length > 0">
      <FileList
        :files="store.files"
        @remove="store.remove"
        @clear="store.clear()"
      />

      <section
        v-if="ready.length > 0"
        class="next"
      >
        <h2 class="section-title">
          {{ t('home.next') }}
        </h2>
        <RouterLink
          v-for="item in ready"
          :key="item.category"
          class="btn btn--primary next__link"
          :to="item.to"
        >
          {{ t(`home.open.${item.category}`, item.count) }}
        </RouterLink>
      </section>
      <p
        v-if="pending.length > 0"
        class="page__subtitle"
      >
        {{ t('home.noOperations', { categories: pending.map((c) => t(`categories.${c}`)).join(', ') }) }}
      </p>
    </template>
  </div>
</template>

<style scoped>
.next {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--space-3);
}

.next__link {
  text-decoration: none;
}
</style>

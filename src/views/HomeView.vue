<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import DropZone from '../components/DropZone.vue'
import FileList from '../components/FileList.vue'
import { api } from '../services/api'
import type { FileInfo } from '../types/FileInfo'
import { errorMessage } from '../utils/errors'
import { baseName } from '../utils/format'

const { t } = useI18n()
const files = ref<FileInfo[]>([])
const errors = ref<string[]>([])

async function addPaths(paths: string[]) {
  errors.value = []
  const results = await Promise.allSettled(paths.map((path) => api.probeFile(path)))
  results.forEach((result, index) => {
    if (result.status === 'fulfilled') {
      if (!files.value.some((f) => f.path === result.value.path)) {
        files.value.push(result.value)
      }
    } else {
      const name = baseName(paths[index] ?? '')
      errors.value.push(`${t('files.addFailed', { name })}: ${errorMessage(t, result.reason)}`)
    }
  })
}

function remove(path: string) {
  files.value = files.value.filter((f) => f.path !== path)
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

    <template v-if="files.length > 0">
      <FileList
        :files="files"
        @remove="remove"
        @clear="files = []"
      />
      <p class="page__subtitle">
        {{ t('home.noOperations') }}
      </p>
    </template>
  </div>
</template>

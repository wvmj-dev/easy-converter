<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { open } from '@tauri-apps/plugin-dialog'
import { useTools } from '../composables/useTools'
import { FEDORA_FULL_FFMPEG, TOOL_INSTALL } from '../data/toolInstall'
import { useSettingsStore } from '../stores/settings'
import { useToolsStore } from '../stores/tools'
import type { ToolId } from '../types/ToolId'
import type { ToolStatus } from '../types/ToolStatus'
import { errorMessage } from '../utils/errors'
import { currentPlatform } from '../utils/platform'

const { t } = useI18n()
const { statuses, loading, refresh } = useTools()
const settingsStore = useSettingsStore()
const toolsStore = useToolsStore()
const platform = currentPlatform()
const error = ref<string | null>(null)

async function guard(action: () => Promise<void>) {
  error.value = null
  try {
    await action()
  } catch (err) {
    error.value = errorMessage(t, err)
  }
}

function customPath(id: ToolId): string | undefined {
  return settingsStore.settings?.toolPaths[id]
}

async function setCustomPath(id: ToolId, path: string | null) {
  const current = settingsStore.settings
  if (!current) return
  const toolPaths = { ...current.toolPaths }
  if (path) {
    toolPaths[id] = path
  } else {
    delete toolPaths[id]
  }
  // The backend re-checks the tools when their paths change; just read the new statuses.
  await settingsStore.save({ ...current, toolPaths })
  await toolsStore.load()
}

async function choosePath(tool: ToolStatus) {
  const selected = await open({ multiple: false, directory: false })
  if (selected) {
    await guard(() => setCustomPath(tool.id, selected))
  }
}

function sourceLabel(tool: ToolStatus): string {
  return tool.source ? t(`tools.source.${tool.source}`) : ''
}
</script>

<template>
  <div class="page">
    <header class="page__header page__header--row">
      <div>
        <h1 class="page__title">
          {{ t('tools.title') }}
        </h1>
        <p class="page__subtitle">
          {{ t('tools.subtitle') }}
        </p>
      </div>
      <button
        type="button"
        class="btn"
        :disabled="loading"
        @click="guard(refresh)"
      >
        {{ loading ? t('common.loading') : t('tools.refresh') }}
      </button>
    </header>

    <p
      v-if="error"
      class="alert alert--danger"
      role="alert"
    >
      {{ error }}
    </p>

    <p v-if="loading && statuses.length === 0">
      {{ t('common.loading') }}
    </p>

    <ul class="tools">
      <li
        v-for="tool in statuses"
        :key="tool.id"
        class="card tool"
      >
        <header class="tool__header">
          <div>
            <h2 class="tool__name">
              {{ tool.name }}
            </h2>
            <p class="tool__purpose">
              {{ t(`tools.purpose.${tool.id}`) }}
            </p>
          </div>
          <div class="tool__badges">
            <span class="badge">{{ tool.required ? t('tools.required') : t('tools.optional') }}</span>
            <span
              class="badge"
              :class="tool.available ? 'badge--success' : 'badge--danger'"
            >
              {{ tool.available ? t('tools.available') : t('tools.missing') }}
            </span>
          </div>
        </header>

        <dl
          v-if="tool.path"
          class="tool__details"
        >
          <template v-if="tool.version">
            <dt>{{ t('tools.version') }}</dt>
            <dd>{{ tool.version }}</dd>
          </template>
          <dt>{{ t('tools.path') }}</dt>
          <dd>
            <code>{{ tool.path }}</code> <span class="tool__source">({{ sourceLabel(tool) }})</span>
          </dd>
          <template v-if="tool.encoders">
            <dt>FFmpeg</dt>
            <dd>{{ t('tools.encoders', { count: tool.encoders.length }) }}</dd>
          </template>
          <template v-if="tool.languages">
            <dt>{{ t('tools.languages') }}</dt>
            <dd>{{ tool.languages.length > 0 ? tool.languages.join(', ') : t('tools.noLanguages') }}</dd>
          </template>
        </dl>

        <p
          v-if="tool.error"
          class="tool__error"
        >
          {{ tool.error }}
        </p>

        <div
          v-if="!tool.available"
          class="tool__install"
        >
          <h3 class="tool__install-title">
            {{ t('tools.install.title') }}
          </h3>
          <template v-if="tool.id === 'pdfium'">
            <p>{{ t('tools.install.bundled') }}</p>
          </template>
          <template v-else-if="platform === 'windows'">
            <p v-if="tool.required">
              {{ t('tools.install.windowsRequired') }}
            </p>
            <p v-else>
              {{ t('tools.install.windowsOptional', { site: TOOL_INSTALL[tool.id].windowsSite }) }}
            </p>
          </template>
          <template v-else>
            <p>{{ t('tools.install.debian') }}</p>
            <pre><code>{{ TOOL_INSTALL[tool.id].debian }}</code></pre>
            <p>{{ t('tools.install.fedora') }}</p>
            <pre><code>{{ TOOL_INSTALL[tool.id].fedora }}</code></pre>
          </template>
        </div>

        <div
          v-if="tool.id === 'ffmpeg' && platform === 'linux' && tool.encoders && !tool.encoders.includes('libx264')"
          class="tool__note"
        >
          <p>{{ t('tools.install.fedoraFfmpeg') }}</p>
          <pre><code>{{ FEDORA_FULL_FFMPEG }}</code></pre>
        </div>

        <div class="tool__actions">
          <button
            type="button"
            class="btn btn--ghost btn--small"
            @click="choosePath(tool)"
          >
            {{ t('tools.choosePath') }}
          </button>
          <button
            v-if="customPath(tool.id)"
            type="button"
            class="btn btn--ghost btn--small"
            @click="guard(() => setCustomPath(tool.id, null))"
          >
            {{ t('tools.clearPath') }}
          </button>
        </div>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.tools {
  display: grid;
  gap: var(--space-3);
  margin: 0;
  padding: 0;
  list-style: none;
}

.tool__header {
  display: flex;
  justify-content: space-between;
  gap: var(--space-3);
}

.tool__name {
  margin: 0;
  font-size: 1rem;
}

.tool__purpose {
  margin: 0;
  color: var(--color-muted);
  font-size: 0.875rem;
}

.tool__badges {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-start;
  justify-content: flex-end;
  gap: var(--space-1);
}

.tool__details {
  display: grid;
  grid-template-columns: max-content 1fr;
  gap: var(--space-1) var(--space-3);
  margin: var(--space-3) 0 0;
  font-size: 0.875rem;
}

.tool__details dt {
  color: var(--color-muted);
}

.tool__details dd {
  margin: 0;
  overflow-wrap: anywhere;
}

.tool__source {
  color: var(--color-muted);
}

.tool__error {
  color: var(--color-danger);
  font-size: 0.875rem;
}

.tool__install,
.tool__note {
  margin-top: var(--space-3);
  font-size: 0.875rem;
}

.tool__install p,
.tool__note p {
  margin: var(--space-2) 0 var(--space-1);
}

.tool__install-title {
  margin: 0;
  font-size: 0.875rem;
}

.tool__actions {
  display: flex;
  gap: var(--space-1);
  margin-top: var(--space-3);
}
</style>

<script setup lang="ts">
import { onMounted } from 'vue'
import { RouterLink, RouterView } from 'vue-router'
import { useI18n } from 'vue-i18n'
import JobPanel from './components/JobPanel.vue'
import { useSettingsStore } from './stores/settings'

const { t } = useI18n()
const settings = useSettingsStore()

onMounted(() => {
  // Applies the saved language; defaults stay in place if loading fails.
  settings.load().catch((err: unknown) => console.error('failed to load settings', err))
})
</script>

<template>
  <div class="layout">
    <nav
      class="sidebar"
      :aria-label="t('nav.label')"
    >
      <p class="sidebar__brand">
        {{ t('app.name') }}
      </p>
      <RouterLink
        class="sidebar__link"
        to="/"
      >
        {{ t('nav.home') }}
      </RouterLink>
      <RouterLink
        class="sidebar__link"
        to="/image"
      >
        {{ t('nav.image') }}
      </RouterLink>
      <RouterLink
        class="sidebar__link"
        to="/pdf"
      >
        {{ t('nav.pdf') }}
      </RouterLink>
      <RouterLink
        class="sidebar__link"
        to="/video"
      >
        {{ t('nav.video') }}
      </RouterLink>
      <RouterLink
        class="sidebar__link"
        to="/audio"
      >
        {{ t('nav.audio') }}
      </RouterLink>
      <RouterLink
        class="sidebar__link"
        to="/tools"
      >
        {{ t('nav.tools') }}
      </RouterLink>
      <RouterLink
        class="sidebar__link"
        to="/settings"
      >
        {{ t('nav.settings') }}
      </RouterLink>
      <RouterLink
        class="sidebar__link"
        to="/about"
      >
        {{ t('nav.about') }}
      </RouterLink>
    </nav>
    <main class="content">
      <RouterView />
    </main>
    <JobPanel />
  </div>
</template>

<style scoped>
.layout {
  display: flex;
  height: 100vh;
}

.sidebar {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  width: 200px;
  flex-shrink: 0;
  padding: var(--space-4) var(--space-3);
  border-right: 1px solid var(--color-border);
  background: var(--color-surface);
}

.sidebar__brand {
  margin: 0 0 var(--space-3);
  padding: 0 var(--space-2);
  font-weight: 700;
}

.sidebar__link {
  padding: var(--space-2);
  border-radius: var(--radius);
  color: var(--color-text);
  text-decoration: none;
}

.sidebar__link:hover {
  background: var(--color-hover);
}

.sidebar__link.router-link-exact-active {
  background: var(--color-accent-soft);
  color: var(--color-accent);
  font-weight: 600;
}

.content {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
}
</style>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'

const { t } = useI18n()
</script>

<template>
  <main class="home">
    <h1>{{ t('app.name') }}</h1>
    <p class="home__title">
      {{ t('home.title') }}
    </p>
    <p class="home__subtitle">
      {{ t('home.subtitle') }}
    </p>
  </main>
</template>

<style scoped>
.home {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 100vh;
  padding: 2rem;
  text-align: center;
}

.home__title {
  font-size: 1.125rem;
}

.home__subtitle {
  color: var(--color-muted);
}
</style>

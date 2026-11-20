<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { MEDIA_QUALITIES } from '../../operations/video'
import type { MediaQuality } from '../../types/MediaQuality'
import ChoiceField from './ChoiceField.vue'

const quality = defineModel<MediaQuality>({ required: true })
const { t } = useI18n()
const choices = computed(() =>
  MEDIA_QUALITIES.map((value) => ({ value, label: t(`video.compress.levels.${value}`) })),
)
</script>

<template>
  <div class="options">
    <ChoiceField
      v-model="quality"
      :legend="t('video.compress.label')"
      name="video-quality"
      :choices="choices"
    />
    <p class="field__help">
      {{ t('video.compress.help') }}
    </p>
  </div>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import type { VideoOptions } from '../../operations/video'
import TimeRangeField from './TimeRangeField.vue'

const options = defineModel<VideoOptions['trim']>({ required: true })
const { t } = useI18n()
</script>

<template>
  <div class="options">
    <TimeRangeField
      id="video-trim"
      v-model="options"
    />
    <label class="radio">
      <input
        v-model="options.accurate"
        type="checkbox"
      >
      {{ t('video.trim.accurate') }}
    </label>
    <p class="field__help">
      {{ t(options.accurate ? 'video.trim.accurateHelp' : 'video.trim.fastHelp') }}
    </p>
  </div>
</template>

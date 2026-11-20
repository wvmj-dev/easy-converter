<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { VIDEO_LIMITS, type VideoOptions } from '../../operations/video'
import TimeRangeField from './TimeRangeField.vue'

const options = defineModel<VideoOptions['toGif']>({ required: true })
const { t } = useI18n()
</script>

<template>
  <div class="options">
    <div class="field">
      <label
        class="field__label"
        for="gif-width"
      >{{ t('video.toGif.width') }}</label>
      <input
        id="gif-width"
        v-model.number="options.width"
        class="input input--number"
        type="number"
        :min="VIDEO_LIMITS.minGifWidth"
        :max="VIDEO_LIMITS.maxGifWidth"
        step="1"
      >
    </div>
    <div class="field">
      <label
        class="field__label"
        for="gif-fps"
      >{{ t('video.toGif.fps') }}</label>
      <input
        id="gif-fps"
        v-model.number="options.fps"
        class="input input--number"
        type="number"
        min="1"
        :max="VIDEO_LIMITS.maxGifFps"
        step="1"
      >
    </div>
    <TimeRangeField
      id="gif"
      v-model="options"
      :help="t('video.toGif.rangeHelp', { max: VIDEO_LIMITS.maxGifMs / 1000 })"
    />
  </div>
</template>

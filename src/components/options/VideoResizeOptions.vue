<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { VIDEO_HEIGHTS, type VideoOptions } from '../../operations/video'

const options = defineModel<VideoOptions['resize']>({ required: true })
const { t } = useI18n()
</script>

<template>
  <div class="field">
    <label
      class="field__label"
      for="video-height"
    >{{ t('video.resize.label') }}</label>
    <select
      id="video-height"
      v-model.number="options.height"
      class="input"
    >
      <option
        v-for="height in VIDEO_HEIGHTS"
        :key="height"
        :value="height"
      >
        {{ t('video.resize.option', { height }) }}
      </option>
    </select>
    <p class="field__help">
      {{ t('video.resize.help') }}
    </p>
  </div>
</template>

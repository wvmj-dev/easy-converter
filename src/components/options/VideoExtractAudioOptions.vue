<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { EXTRACT_FORMATS, type VideoOptions } from '../../operations/video'
import ChoiceField from './ChoiceField.vue'

const options = defineModel<VideoOptions['extractAudio']>({ required: true })
const { t } = useI18n()
const formats = computed(() =>
  EXTRACT_FORMATS.map((value) => ({ value, label: t(`media.audioFormats.${value}`) })),
)
</script>

<template>
  <div class="options">
    <ChoiceField
      v-model="options.format"
      :legend="t('media.audioFormat')"
      name="video-audio-format"
      :choices="formats"
    />
    <p class="field__help">
      {{ t('video.extractAudio.help') }}
    </p>
  </div>
</template>

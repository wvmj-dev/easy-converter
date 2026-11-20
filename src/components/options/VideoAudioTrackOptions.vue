<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { open } from '@tauri-apps/plugin-dialog'
import type { VideoOptions } from '../../operations/video'
import { baseName } from '../../utils/format'
import ChoiceField from './ChoiceField.vue'

/** Extensions offered when choosing the new audio (the backend checks the file again). */
const AUDIO_EXTENSIONS = ['mp3', 'wav', 'flac', 'ogg', 'oga', 'opus', 'm4a', 'aac', 'wma', 'aiff', 'aif', 'mka']

const options = defineModel<VideoOptions['audioTrack']>({ required: true })
const { t } = useI18n()
const modes = computed(() =>
  (['remove', 'replace'] as const).map((value) => ({ value, label: t(`video.audioTrack.modes.${value}`) })),
)

async function chooseAudio() {
  const selected = await open({
    multiple: false,
    directory: false,
    filters: [{ name: t('video.audioTrack.filter'), extensions: AUDIO_EXTENSIONS }],
  })
  if (typeof selected === 'string') options.value.audio = selected
}
</script>

<template>
  <div class="options">
    <ChoiceField
      v-model="options.mode"
      :legend="t('video.audioTrack.label')"
      name="video-audio-mode"
      :choices="modes"
    />
    <div
      v-if="options.mode === 'replace'"
      class="field"
    >
      <button
        type="button"
        class="btn"
        @click="chooseAudio"
      >
        {{ t('video.audioTrack.choose') }}
      </button>
      <p class="field__help">
        {{ options.audio ? baseName(options.audio) : t('video.audioTrack.none') }}
      </p>
      <p class="field__help">
        {{ t('video.audioTrack.replaceHelp') }}
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { AUDIO_BITRATES, AUDIO_FORMATS, LOSSLESS_FORMATS, type AudioOptions } from '../../operations/audio'
import ChoiceField from './ChoiceField.vue'

const options = defineModel<AudioOptions['convert']>({ required: true })
const { t } = useI18n()
const formats = computed(() =>
  AUDIO_FORMATS.map((value) => ({ value, label: t(`media.audioFormats.${value}`) })),
)
const lossy = computed(() => !LOSSLESS_FORMATS.includes(options.value.format))
</script>

<template>
  <div class="options">
    <ChoiceField
      v-model="options.format"
      :legend="t('media.audioFormat')"
      name="audio-format"
      :choices="formats"
    />
    <div
      v-if="lossy"
      class="field"
    >
      <label
        class="field__label"
        for="audio-bitrate"
      >{{ t('audio.convert.bitrate') }}</label>
      <select
        id="audio-bitrate"
        v-model="options.bitrate"
        class="input"
      >
        <option
          v-for="bitrate in AUDIO_BITRATES"
          :key="bitrate"
          :value="bitrate"
        >
          {{ bitrate === 'auto' ? t('audio.convert.auto') : t('audio.convert.kbps', { n: bitrate }) }}
        </option>
      </select>
      <p class="field__help">
        {{ t('audio.convert.bitrateHelp') }}
      </p>
    </div>
  </div>
</template>

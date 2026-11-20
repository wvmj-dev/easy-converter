<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { VIDEO_CONTAINERS, type VideoOptions } from '../../operations/video'
import ChoiceField from './ChoiceField.vue'

const options = defineModel<VideoOptions['convert']>({ required: true })
const { t } = useI18n()
const containers = computed(() =>
  VIDEO_CONTAINERS.map((value) => ({ value, label: t(`video.containers.${value}`) })),
)
</script>

<template>
  <div class="options">
    <ChoiceField
      v-model="options.container"
      :legend="t('video.convert.label')"
      name="video-container"
      :choices="containers"
    />
    <p class="field__help">
      {{ t('video.convert.help') }}
    </p>
  </div>
</template>
